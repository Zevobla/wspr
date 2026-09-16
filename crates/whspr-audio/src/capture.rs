//! Live microphone capture: opening an input stream, buffering samples
//! from it, and turning the result into a canonical 16kHz mono
//! `AudioBuffer` on `stop()`.
//!
//! `start_capture`/`start_capture_on_device` are thin, zero-/one-argument
//! convenience wrappers around `start_capture_with`, which takes a full
//! `CaptureOptions` (device selection, gain, noise suppression, preroll).

use std::sync::{Arc, Mutex};

use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;

use whspr_core::{AudioBuffer, Result, WhsprError};

use crate::device;

/// Options for `start_capture_with`: which device to open, plus how the
/// captured audio is post-processed when `CaptureHandle::stop` is later
/// called.
///
/// `Default` reproduces the exact behavior `start_capture`/
/// `start_capture_on_device` have always had: the OS default device, no
/// gain change, no noise suppression, no preroll prepended.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// Input device name to open, resolved the same way as
    /// `start_capture_on_device` (falls back to the OS default if no
    /// device matches this name). `None` opens the OS default directly.
    pub device: Option<String>,
    /// Linear gain multiplier applied to every sample on `stop()` (see
    /// `apply_gain`). `1.0` is a no-op — this mirrors `whspr-config`'s
    /// `[capture] input_gain` default.
    pub input_gain: f32,
    /// Whether `stop()` runs `suppress_noise` on the captured audio.
    /// Mirrors `whspr-config`'s `[capture] noise_suppression`.
    pub noise_suppression: bool,
    /// Samples to prepend to the captured audio on `stop()`, already at
    /// 16kHz mono — the same shape `PrerollMonitor::snapshot` and
    /// `PrerollBuffer::drain_preroll` produce. Empty is a no-op.
    pub preroll: Vec<f32>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        CaptureOptions {
            device: None,
            input_gain: 1.0,
            noise_suppression: false,
            preroll: Vec::new(),
        }
    }
}

/// Handle for an in-progress microphone capture session.
///
/// Holds the cpal stream, shared sample buffer, and device sample rate,
/// plus the `CaptureOptions` fields that `stop()` needs to apply its
/// post-processing pipeline. The buffer is always mono at the device's
/// native sample rate — a multi-channel device is downmixed to mono
/// per-callback (see `crate::stream::build_mono_input_stream`), never
/// stored as raw interleaved frames.
pub struct CaptureHandle {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    preroll: Vec<f32>,
    input_gain: f32,
    noise_suppression: bool,
}

impl CaptureHandle {
    /// Stops capture and returns the recorded audio as a 16kHz mono
    /// buffer, after running the full post-processing pipeline, applied
    /// in this exact order:
    ///
    /// 1. Resample the raw captured audio to 16kHz mono (the canonical
    ///    shape — unconditional, as it always has been).
    /// 2. Prepend `preroll` (already 16kHz mono) to the front of the
    ///    resampled samples, if any was configured.
    /// 3. Apply `input_gain` (see `apply_gain`).
    /// 4. If `noise_suppression` is on, run `suppress_noise`.
    ///
    /// With `CaptureOptions::default()` (what `start_capture`/
    /// `start_capture_on_device` use), steps 2-4 are all no-ops, so the
    /// output is unchanged from before this pipeline existed.
    pub fn stop(self) -> Result<AudioBuffer> {
        drop(self.stream);

        // Acquire the mutex and clone the captured samples
        let samples = self
            .buffer
            .lock()
            .map_err(|e| WhsprError::Audio(format!("failed to lock capture buffer: {}", e)))?
            .clone();

        // Wrap in an AudioBuffer at the device's native sample rate, then
        // resample to 16kHz mono (the canonical shape) - step 1.
        let native = AudioBuffer::new(samples, self.sample_rate);
        let resampled = crate::resample_to_16k_mono(&native)?;
        let mut samples = resampled.samples;

        // Step 2: prepend preroll.
        if !self.preroll.is_empty() {
            let mut combined = self.preroll;
            combined.extend_from_slice(&samples);
            samples = combined;
        }

        // Step 3: gain.
        crate::apply_gain(&mut samples, self.input_gain);

        // Step 4: noise suppression.
        if self.noise_suppression {
            crate::suppress_noise(&mut samples, 16000);
        }

        Ok(AudioBuffer::new(samples, 16000))
    }

    /// RMS input level (~0.0..1.0) over the most recent ~100ms, for a meter.
    pub fn current_level(&self) -> f32 {
        let window = (self.sample_rate / 10).max(1) as usize;
        let Ok(samples) = self.buffer.lock() else {
            return 0.0;
        };
        let slice = &samples[samples.len().saturating_sub(window)..];
        if slice.is_empty() {
            return 0.0;
        }
        (slice.iter().map(|s| s * s).sum::<f32>() / slice.len() as f32).sqrt()
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
pub(crate) fn no_input_device_error() -> WhsprError {
    WhsprError::Audio(format!(
        "no microphone available or access denied: {MIC_RECOVERY_STEPS}"
    ))
}

/// Builds the error for a cpal call that failed on an otherwise-present
/// input device (`default_input_config`, `build_input_stream`, `play`).
/// These failures are the ones a denied OS mic-permission prompt actually
/// surfaces as (unlike e.g. an unsupported sample format, which is a real
/// device with a genuine format mismatch, not a permission problem).
pub(crate) fn mic_access_error(action: &str, cause: impl std::fmt::Display) -> WhsprError {
    WhsprError::Audio(format!(
        "failed to {action}: {cause} (no microphone available or access \
         denied: {MIC_RECOVERY_STEPS})"
    ))
}

/// Starts recording from the default input device. Equivalent to
/// `start_capture_on_device(None)`, kept as its own zero-argument entry
/// point so existing callers don't need to change just because device
/// selection (C-05) exists now. Equivalent to
/// `start_capture_with(CaptureOptions::default())`.
///
/// Returns a `CaptureHandle` that can be stopped to retrieve the recorded audio
/// as a 16kHz mono buffer.
pub fn start_capture() -> Result<CaptureHandle> {
    start_capture_with(CaptureOptions::default())
}

/// Starts recording from `device` by name if given (falling back to the
/// default input device if no device matches that name - see
/// `resolve_input_device`), or the default input device directly if
/// `device` is `None`. C-05: lets a caller honor a user's chosen input
/// device (e.g. the Hub's device picker) instead of always opening
/// whatever the OS considers "default". Equivalent to
/// `start_capture_with(CaptureOptions { device: device.map(str::to_string), ..Default::default() })`.
pub fn start_capture_on_device(device: Option<&str>) -> Result<CaptureHandle> {
    start_capture_with(CaptureOptions {
        device: device.map(|d| d.to_string()),
        ..CaptureOptions::default()
    })
}

/// Starts recording using `opts` — the option-ful entry point that
/// `start_capture`/`start_capture_on_device` are thin wrappers around.
/// See `CaptureOptions` for what each field controls and `CaptureHandle::stop`
/// for the exact order they're applied in.
pub fn start_capture_with(opts: CaptureOptions) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = match opts.device.as_deref() {
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

    // Use Arc<Mutex> to share the sample buffer between the audio callback and main thread
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_clone = Arc::clone(&buffer);

    // Downmixes whatever sample format/channel count the device reports
    // to mono f32 (see crate::stream) before appending to the buffer -
    // stop() below expects mono samples at `sample_rate`.
    let stream = crate::stream::build_mono_input_stream(
        &device,
        &config,
        move |mono: &[f32]| {
            if let Ok(mut buf) = buffer_clone.lock() {
                buf.extend_from_slice(mono);
            }
        },
        "capture",
    )?;

    // Begin audio capture by playing the input stream
    stream
        .play()
        .map_err(|e| mic_access_error("start capture stream", e))?;

    Ok(CaptureHandle {
        stream,
        buffer,
        sample_rate,
        preroll: opts.preroll,
        input_gain: opts.input_gain,
        noise_suppression: opts.noise_suppression,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_options_default_matches_legacy_start_capture_behavior() {
        let opts = CaptureOptions::default();
        assert_eq!(opts.device, None);
        assert_eq!(opts.input_gain, 1.0, "1.0 gain must be a no-op");
        assert!(
            !opts.noise_suppression,
            "noise suppression off by default, matching whspr-config's default"
        );
        assert!(opts.preroll.is_empty());
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
