//! Audio capture, decoding, and resampling. Signatures are settled here so
//! the rest of the workspace can depend on them; bodies are `todo!()` until
//! the audio team opts in `hound`/`rubato`/`cpal` from this crate's own
//! Cargo.toml — this crate must keep compiling without those in the
//! meantime.

mod device;
mod preroll;

pub use device::input_device_names;
pub use preroll::{PrerollBuffer, DEFAULT_PREROLL_MS};

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

/// Helper to convert a WAV sample read error into a WhsprError.
fn wav_read_err(e: impl std::fmt::Display) -> WhsprError {
    WhsprError::Audio(format!("failed to read WAV sample: {}", e))
}

/// Trims leading and trailing silence from `audio`.
///
/// Classifies the audio in fixed ~20ms windows (the window length is
/// derived from `audio.sample_rate`, so this is correct at any input rate,
/// not just 16kHz) by RMS energy: a window is "silent" if its RMS is `<=
/// threshold`. Whole silent windows are dropped from the very start and
/// very end only; speech in the middle — including a pause *inside* an
/// utterance — is never touched, because the scan from each end stops as
/// soon as it hits one non-silent window.
///
/// `min_keep` is a floor, in samples, on the trimmed output's length: if
/// trimming would leave fewer than `min_keep` samples — including the
/// degenerate case of an entirely-silent buffer, where the forward and
/// backward scans would otherwise meet in the middle and trim everything —
/// the original `audio` is returned unchanged instead of over-trimming
/// toward empty. This is what makes it safe to call unconditionally from
/// `decode_wav`: it never hands a downstream ASR backend a surprise empty
/// clip, and a buffer shorter than one window is also returned unchanged
/// (there's nothing safe to window-classify).
pub fn trim_silence(audio: &AudioBuffer, threshold: f32, min_keep: usize) -> AudioBuffer {
    let window_len = ((audio.sample_rate as usize) / 50).max(1); // ~20ms
    let samples = &audio.samples;
    let n = samples.len();

    if n < window_len {
        return audio.clone();
    }

    let window_rms = |start: usize| -> f32 {
        let end = (start + window_len).min(n);
        let window = &samples[start..end];
        (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt()
    };

    let mut start = 0;
    while start + window_len <= n && window_rms(start) <= threshold {
        start += window_len;
    }

    let mut end = n;
    while end >= window_len
        && end - window_len >= start
        && window_rms(end - window_len) <= threshold
    {
        end -= window_len;
    }

    if start >= end || end - start < min_keep {
        return audio.clone();
    }

    AudioBuffer::new(samples[start..end].to_vec(), audio.sample_rate)
}

/// Default RMS energy threshold (on the `AudioBuffer`'s [-1.0, 1.0]
/// amplitude scale) below which a short window of audio is classified as
/// silence by `trim_silence`.
pub const DEFAULT_SILENCE_THRESHOLD: f32 = 0.02;

/// Default floor, in samples, below which `trim_silence` refuses to shrink
/// the buffer further. 1600 samples is 100ms at 16kHz. Guards against ever
/// handing an ASR backend a suspiciously tiny or fully-empty clip.
pub const DEFAULT_MIN_KEEP_SAMPLES: usize = 1600;

/// `trim_silence` with sensible defaults (see `DEFAULT_SILENCE_THRESHOLD`,
/// `DEFAULT_MIN_KEEP_SAMPLES`). This is what `decode_wav` calls internally.
pub fn trim_silence_default(audio: &AudioBuffer) -> AudioBuffer {
    trim_silence(audio, DEFAULT_SILENCE_THRESHOLD, DEFAULT_MIN_KEEP_SAMPLES)
}

/// Decodes a WAV file into an `AudioBuffer` at its native sample rate/channel
/// layout (resample separately with `resample_to_16k_mono`).
///
/// Automatically trims leading and trailing silence (see `trim_silence` for details).
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
                let sample = frame.map_err(wav_read_err)?;
                // i8: -128..127 -> -1.0..1.0
                let normalized = sample as f32 / 128.0;
                samples.push(normalized);
            }
        }
        16 => {
            for frame in reader.samples::<i16>() {
                let sample = frame.map_err(wav_read_err)?;
                // i16: -32768..32767 -> -1.0..1.0
                let normalized = sample as f32 / 32768.0;
                samples.push(normalized);
            }
        }
        24 => {
            for frame in reader.samples::<i32>() {
                let sample = frame.map_err(wav_read_err)?;
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
                    let sample = frame.map_err(wav_read_err)?;
                    samples.push(sample.clamp(-1.0, 1.0));
                }
            } else {
                // 32-bit int
                for frame in reader.samples::<i32>() {
                    let sample = frame.map_err(wav_read_err)?;
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

    let audio = AudioBuffer::new(samples, sample_rate);
    Ok(trim_silence_default(&audio))
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
        drop(self.stream);

        // Acquire the mutex and clone the captured samples
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

/// Concrete, OS-specific next step for a `start_capture` failure that's
/// consistent with "no microphone" or "access denied" - a missing/
/// unplugged device and a denied OS permission prompt look identical from
/// cpal's point of view, so this same guidance covers both (C-13: name a
/// recovery step, not just "capture failed").
const MIC_RECOVERY_STEPS: &str = "grant microphone access (macOS: System \
    Settings → Privacy & Security → Microphone; Linux: check your audio \
    server/device permissions) and reconnect an input device, then retry";

/// Builds the error for `host.default_input_device()` returning `None` -
/// no cpal error to wrap here, just an absent device.
fn no_input_device_error() -> WhsprError {
    WhsprError::Audio(format!(
        "no microphone available or access denied: {MIC_RECOVERY_STEPS}"
    ))
}

/// Builds the error for a cpal call that failed on an otherwise-present
/// input device (`default_input_config`, `build_input_stream`, `play`).
/// These failures are the ones a denied OS mic-permission prompt actually
/// surfaces as (unlike e.g. an unsupported sample format, which is a real
/// device with a genuine format mismatch, not a permission problem).
fn mic_access_error(action: &str, cause: impl std::fmt::Display) -> WhsprError {
    WhsprError::Audio(format!(
        "failed to {action}: {cause} (no microphone available or access \
         denied: {MIC_RECOVERY_STEPS})"
    ))
}

/// Starts recording from the default input device. Equivalent to
/// `start_capture_on_device(None)`, kept as its own zero-argument entry
/// point so existing callers don't need to change just because device
/// selection (C-05) exists now.
///
/// Returns a `CaptureHandle` that can be stopped to retrieve the recorded audio
/// as a 16kHz mono buffer.
pub fn start_capture() -> Result<CaptureHandle> {
    start_capture_on_device(None)
}

/// Starts recording from `device` by name if given (falling back to the
/// default input device if no device matches that name - see
/// `resolve_input_device`), or the default input device directly if
/// `device` is `None`. C-05: lets a caller honor a user's chosen input
/// device (e.g. the Hub's device picker) instead of always opening
/// whatever the OS considers "default".
pub fn start_capture_on_device(device: Option<&str>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = match device {
        Some(name) => device::resolve_input_device(&host, name)?,
        None => host
            .default_input_device()
            .ok_or_else(no_input_device_error)?,
    };

    // Query device for its preferred configuration
    let config = device
        .default_input_config()
        .map_err(|e| mic_access_error("get default input config", e))?;

    let sample_rate = config.sample_rate();
    let stream_config: StreamConfig = config.into();

    // Use Arc<Mutex> to share the sample buffer between the audio callback and main thread
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_clone = Arc::clone(&buffer);

    // Create a stream callback to route audio from the device to our buffer
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
            .map_err(|e| mic_access_error("build F32 input stream", e))?,
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
                .map_err(|e| mic_access_error("build I16 input stream", e))?
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
                .map_err(|e| mic_access_error("build U16 input stream", e))?
        }
        _ => {
            return Err(WhsprError::Audio(format!(
                "unsupported sample format: {:?}",
                config.sample_format()
            )));
        }
    };

    // Begin audio capture by playing the input stream
    stream
        .play()
        .map_err(|e| mic_access_error("start capture stream", e))?;

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

    // C-13: a mic-failure error must name a concrete recovery step, not
    // just say "capture failed". These test the two error-message builders
    // directly rather than the live-capture entry point itself, which
    // needs real (or absent) audio hardware to exercise deterministically
    // and, per AB-06, must never be called from test code.

    #[test]
    fn no_input_device_error_names_recovery_steps() {
        let msg = no_input_device_error().to_string();
        let lower = msg.to_lowercase();
        assert!(lower.contains("microphone"), "message was: {msg:?}");
        assert!(
            lower.contains("access") && msg.contains("Settings"),
            "message was: {msg:?}"
        );
    }

    #[test]
    fn mic_access_error_includes_action_cause_and_recovery_steps() {
        let msg = mic_access_error("get default input config", "permission denied").to_string();
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("get default input config"),
            "message was: {msg:?}"
        );
        assert!(lower.contains("permission denied"), "message was: {msg:?}");
        assert!(lower.contains("microphone"), "message was: {msg:?}");
        assert!(
            lower.contains("access") && msg.contains("Settings"),
            "message was: {msg:?}"
        );
    }
}
