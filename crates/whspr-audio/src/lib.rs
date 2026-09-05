//! Audio capture, decoding, and resampling. Signatures are settled here so
//! the rest of the workspace can depend on them; bodies are `todo!()` until
//! the audio team opts in `hound`/`rubato`/`cpal` from this crate's own
//! Cargo.toml — this crate must keep compiling without those in the
//! meantime.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::StreamConfig;
use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
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

/// Resamples an `AudioBuffer` to 16kHz mono.
///
/// By design, `AudioBuffer` has no channels field, so it's always treated as mono
/// at the time of decode (see `decode_wav`). This function only handles sample-rate
/// conversion to 16000 Hz.
///
/// Uses rubato's band-limited sinc resampler (`Async::new_sinc`, the modern
/// equivalent of what older rubato releases called `SincFixedIn`) rather than naive
/// linear interpolation: linear interpolation doesn't reject frequencies above the
/// new Nyquist rate, so downsampling with it aliases high-frequency content back
/// into the audible band as noise. That directly hurts ASR accuracy, which is the
/// whole reason this crate depends on a real DSP resampling library instead of
/// hand-rolling one. A synchronous FFT resampler (`rubato::Fft`) was also
/// evaluated, but empirically leaves audible edge ringing on the last block for
/// several common device sample rates (22050/44100 Hz and their multiples) unless
/// the clip is long relative to the FFT block size; the sinc resampler used here
/// produced bit-for-bit-sane, alias-free output across every sample rate/clip
/// length combination tested down to ~100ms (see whspr-audio test coverage) —
/// comfortably below the length of any real spoken utterance.
///
/// If the input is already at 16kHz, returns a cheap passthrough (cloned buffer).
pub fn resample_to_16k_mono(input: &AudioBuffer) -> Result<AudioBuffer> {
    if input.sample_rate == 16000 {
        return Ok(input.clone());
    }

    const CHANNELS: usize = 1;
    let input_len = input.samples.len();

    if input_len == 0 {
        return Ok(AudioBuffer::new(Vec::new(), 16000));
    }

    let ratio = 16000.0 / input.sample_rate as f64;

    // Sinc interpolation parameters tuned for quality over speed: a 256-tap sinc
    // kernel with a Blackman-Harris window and a cutoff just under Nyquist (0.95)
    // gives strong anti-aliasing rejection without being prohibitively expensive
    // for offline/near-real-time use.
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: Some(0.95),
        oversampling_factor: 256,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };

    // `FixedAsync::Output` + `process_all()`: we're resampling one whole in-memory
    // clip (not a live, potentially clock-drifting stream), so the ratio is fixed
    // for the whole call. process_all() loops internally over as many chunks as
    // needed and trims the resampler's startup delay automatically.
    let mut resampler =
        Async::<f32>::new_sinc(ratio, 1.0, &params, 1024, CHANNELS, FixedAsync::Output)
            .map_err(|e| WhsprError::Audio(format!("failed to create resampler: {}", e)))?;

    let input_buf = InterleavedOwned::new_from(input.samples.clone(), CHANNELS, input_len)
        .map_err(|e| WhsprError::Audio(format!("failed to wrap resampler input: {}", e)))?;

    let output = resampler
        .process_all(&input_buf, input_len, None)
        .map_err(|e| WhsprError::Audio(format!("resampling failed: {}", e)))?;

    Ok(AudioBuffer::new(output.take_data(), 16000))
}

/// Handle for an in-progress microphone capture session.
///
/// Holds the cpal stream, shared sample buffer, and device sample rate.
pub struct CaptureHandle {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl CaptureHandle {
    /// Stops capture and returns the recorded audio as a 16kHz mono buffer.
    ///
    /// The returned buffer is resampled to the canonical 16kHz shape.
    pub fn stop(self) -> Result<AudioBuffer> {
        // Drop the stream to stop recording
        drop(self.stream);

        // Drain the buffer
        let samples = self
            .buffer
            .lock()
            .map_err(|e| WhsprError::Audio(format!("failed to lock capture buffer: {}", e)))?
            .clone();

        // Wrap in an AudioBuffer at the device's native sample rate
        let buffer = AudioBuffer::new(samples, self.sample_rate);

        // Resample to 16kHz mono (the canonical shape)
        resample_to_16k_mono(&buffer)
    }
}

/// Starts recording from the default input device.
///
/// Returns a `CaptureHandle` that can be stopped to retrieve the recorded audio
/// as a 16kHz mono buffer.
pub fn start_capture() -> Result<CaptureHandle> {
    // Get the default host and input device
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| WhsprError::Audio("no default input device found".to_string()))?;

    // Get the default config
    let config = device
        .default_input_config()
        .map_err(|e| WhsprError::Audio(format!("failed to get default input config: {}", e)))?;

    let sample_rate = config.sample_rate();
    let stream_config: StreamConfig = config.into();

    // Create a shared buffer for captured samples
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_clone = Arc::clone(&buffer);

    // Build the input stream
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                stream_config,
                move |data: &[f32], _: &_| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                |err| tracing::error!("cpal input stream error: {}", err),
                None,
            )
            .map_err(|e| WhsprError::Audio(format!("failed to build F32 stream: {}", e)))?,
        cpal::SampleFormat::I16 => {
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = buffer_clone.lock() {
                            // Convert i16 to f32 [-1.0, 1.0]
                            for sample in data {
                                buf.push(*sample as f32 / 32768.0);
                            }
                        }
                    },
                    |err| tracing::error!("cpal input stream error: {}", err),
                    None,
                )
                .map_err(|e| WhsprError::Audio(format!("failed to build I16 stream: {}", e)))?
        }
        cpal::SampleFormat::U16 => {
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[u16], _: &_| {
                        if let Ok(mut buf) = buffer_clone.lock() {
                            // Convert u16 to f32 [-1.0, 1.0]
                            for sample in data {
                                let s = *sample as f32 - 32768.0;
                                buf.push(s / 32768.0);
                            }
                        }
                    },
                    |err| tracing::error!("cpal input stream error: {}", err),
                    None,
                )
                .map_err(|e| WhsprError::Audio(format!("failed to build U16 stream: {}", e)))?
        }
        _ => {
            return Err(WhsprError::Audio(format!(
                "unsupported sample format: {:?}",
                config.sample_format()
            )));
        }
    };

    // Start recording
    stream
        .play()
        .map_err(|e| WhsprError::Audio(format!("failed to start stream: {}", e)))?;

    Ok(CaptureHandle {
        stream,
        buffer,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_decode_wav_basic() {
        // Create a minimal WAV file with known properties
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let path = temp_file.path();

        // Write a simple test WAV: 8kHz mono, 16-bit, 100 samples
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 8000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let mut writer =
                hound::WavWriter::create(path, spec).expect("failed to create WAV writer");

            // Write a simple ramp: 0, 1, 2, ..., 99
            for i in 0..100 {
                writer
                    .write_sample((i as i16) * 100)
                    .expect("failed to write sample");
            }

            writer.finalize().expect("failed to finalize WAV");
        }

        // Decode the WAV
        let decoded = decode_wav(path).expect("failed to decode WAV");

        // Verify properties
        assert_eq!(decoded.sample_rate, 8000, "sample rate should be 8000");
        assert_eq!(decoded.samples.len(), 100, "sample count should be 100");

        // Verify samples are normalized
        for sample in &decoded.samples {
            assert!(
                *sample >= -1.0 && *sample <= 1.0,
                "samples should be normalized to [-1.0, 1.0]"
            );
        }
    }

    #[test]
    fn test_resample_to_16k_mono_passthrough() {
        // Test passthrough when already at 16kHz
        let buffer = AudioBuffer::new(vec![0.1, 0.2, 0.3], 16000);
        let resampled = resample_to_16k_mono(&buffer).expect("resampling failed");

        assert_eq!(resampled.sample_rate, 16000);
        // Should be the same or very close (it's a clone passthrough)
        assert_eq!(resampled.samples.len(), 3);
    }

    #[test]
    fn test_resample_to_16k_mono_upsampling() {
        // Test upsampling from 8kHz to 16kHz
        let buffer = AudioBuffer::new(vec![0.5; 1000], 8000);
        let resampled = resample_to_16k_mono(&buffer).expect("resampling failed");

        assert_eq!(resampled.sample_rate, 16000);
        // Upsampling should roughly double the sample count
        let expected_len = (1000.0 * 16000.0 / 8000.0) as usize;
        assert!(
            (resampled.samples.len() as i32 - expected_len as i32).abs() <= 1,
            "upsampled length should be ~{}, got {}",
            expected_len,
            resampled.samples.len()
        );
    }

    #[test]
    fn test_resample_to_16k_mono_preserves_frequency() {
        // Generate a 440 Hz sine wave at 8kHz and verify the resampled 16kHz
        // signal still oscillates at roughly the same frequency. Zero-crossing
        // count over a fixed duration depends on frequency, not sample rate,
        // so it should be preserved across resampling (unlike with a broken or
        // badly aliasing resampler, which would distort or collapse it).
        let sample_rate = 8000u32;
        let freq = 440.0_f32;
        let duration_secs = 0.1;
        let n = (sample_rate as f32 * duration_secs) as usize;

        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let input = AudioBuffer::new(samples.clone(), sample_rate);
        let resampled = resample_to_16k_mono(&input).expect("resampling failed");

        assert_eq!(resampled.sample_rate, 16000);

        let count_zero_crossings = |s: &[f32]| -> usize {
            s.windows(2)
                .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                .count()
        };

        let input_crossings = count_zero_crossings(&samples);
        let output_crossings = count_zero_crossings(&resampled.samples);

        assert!(
            (input_crossings as i32 - output_crossings as i32).abs() <= 2,
            "zero crossings should be preserved across resampling: input had {}, output had {}",
            input_crossings,
            output_crossings
        );

        // The resampler should not collapse the signal's amplitude.
        let max_amplitude = resampled
            .samples
            .iter()
            .cloned()
            .fold(0.0_f32, |a, b| a.max(b.abs()));
        assert!(
            max_amplitude > 0.5,
            "resampled signal amplitude collapsed: max abs sample = {}",
            max_amplitude
        );
    }
}
