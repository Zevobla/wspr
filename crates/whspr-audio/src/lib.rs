//! Audio capture, decoding, and resampling. Signatures are settled here so
//! the rest of the workspace can depend on them; bodies are `todo!()` until
//! the audio team opts in `hound`/`rubato`/`cpal` from this crate's own
//! Cargo.toml — this crate must keep compiling without those in the
//! meantime.

use std::path::Path;

use whspr_core::{AudioBuffer, Result};

/// Decodes a WAV file into an `AudioBuffer` at its native sample rate/channel
/// layout (resample separately with `resample_to_16k_mono`).
pub fn decode_wav(path: impl AsRef<Path>) -> Result<AudioBuffer> {
    let _ = path;
    todo!("whspr-audio: decode_wav via hound")
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
