//! Audio capture, decoding, and resampling. Signatures are settled here so
//! the rest of the workspace can depend on them; bodies are `todo!()` until
//! the audio team opts in `hound`/`rubato`/`cpal` from this crate's own
//! Cargo.toml — this crate must keep compiling without those in the
//! meantime.

use std::path::Path;

use whspr_core::{AudioBuffer, Result, WhsprError};

/// Decodes a WAV file into an `AudioBuffer` at its native sample rate/channel
/// layout (resample separately with `resample_to_16k_mono`).
///
/// Handles arbitrary bit depths (8/16/24/32-bit int, float) and channel layouts,
/// normalizing all to f32 [-1.0, 1.0] and downmixing to mono during decode.
/// (By design, `AudioBuffer` has no channels field, so it's always implicitly mono.)
pub fn decode_wav(path: impl AsRef<Path>) -> Result<AudioBuffer> {
    let path = path.as_ref();
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| WhsprError::Audio(format!("failed to open WAV file: {}", e)))?;

    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    let mut samples = Vec::new();

    match spec.bits_per_sample {
        8 => {
            // 8-bit uses signed i8
            for frame in reader.samples::<i8>() {
                let sample = frame
                    .map_err(|e| WhsprError::Audio(format!("failed to read WAV sample: {}", e)))?;
                // i8: -128..127 -> -1.0..1.0
                let normalized = sample as f32 / 128.0;
                samples.push(normalized);
            }
        }
        16 => {
            for frame in reader.samples::<i16>() {
                let sample = frame
                    .map_err(|e| WhsprError::Audio(format!("failed to read WAV sample: {}", e)))?;
                // i16: -32768..32767 -> -1.0..1.0
                let normalized = sample as f32 / 32768.0;
                samples.push(normalized);
            }
        }
        24 => {
            for frame in reader.samples::<i32>() {
                let sample = frame
                    .map_err(|e| WhsprError::Audio(format!("failed to read WAV sample: {}", e)))?;
                // hound treats 24-bit as i32; shift right by 8 to get actual 24-bit value
                let sample_24 = sample >> 8;
                // i24: -8388608..8388607 -> -1.0..1.0
                let normalized = sample_24 as f32 / 8388608.0;
                samples.push(normalized);
            }
        }
        32 => {
            // Check if float or int format
            if spec.sample_format == hound::SampleFormat::Float {
                for frame in reader.samples::<f32>() {
                    let sample = frame.map_err(|e| {
                        WhsprError::Audio(format!("failed to read WAV sample: {}", e))
                    })?;
                    samples.push(sample.clamp(-1.0, 1.0));
                }
            } else {
                // 32-bit int
                for frame in reader.samples::<i32>() {
                    let sample = frame.map_err(|e| {
                        WhsprError::Audio(format!("failed to read WAV sample: {}", e))
                    })?;
                    // i32: -2147483648..2147483647 -> -1.0..1.0
                    let normalized = sample as f32 / 2147483648.0;
                    samples.push(normalized.clamp(-1.0, 1.0));
                }
            }
        }
        other => {
            return Err(WhsprError::Audio(format!(
                "unsupported bit depth: {}",
                other
            )))
        }
    }

    // Downmix to mono by averaging channels
    if channels > 1 {
        let mut mono = Vec::new();
        for frame_idx in 0..(samples.len() / channels) {
            let mut sum = 0.0;
            for ch in 0..channels {
                sum += samples[frame_idx * channels + ch];
            }
            mono.push(sum / channels as f32);
        }
        samples = mono;
    }

    Ok(AudioBuffer::new(samples, sample_rate))
}

/// Resamples and downmixes an arbitrary `AudioBuffer` to 16kHz mono, the
/// shape every `AsrBackend` expects.
pub fn resample_to_16k_mono(input: &AudioBuffer) -> Result<AudioBuffer> {
    let _ = input;
    todo!("whspr-audio: resample via rubato")
}

/// Handle for an in-progress microphone capture session.
pub struct CaptureHandle;

impl CaptureHandle {
    /// Stops capture and returns the recorded audio as a 16kHz mono buffer.
    pub fn stop(self) -> Result<AudioBuffer> {
        todo!("whspr-audio: stop cpal stream and drain captured samples")
    }
}

/// Starts recording from the default input device.
pub fn start_capture() -> Result<CaptureHandle> {
    todo!("whspr-audio: start_capture via cpal")
}
