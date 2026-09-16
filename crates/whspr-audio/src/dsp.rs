//! Pure signal-processing helpers applied to already-captured audio:
//! linear gain and a lightweight, honest noise-reduction chain. Both
//! operate in place on `f32` sample slices and don't assume any
//! particular caller (`CaptureHandle::stop` is the one in this crate, via
//! `CaptureOptions`), so they're unit-tested directly against synthetic
//! waveforms rather than live audio hardware.

/// Applies a linear gain multiplier to every sample in `samples`, clamping
/// the result to `[-1.0, 1.0]` so a large `gain` can't overflow the audio
/// buffer's normalized amplitude range.
///
/// `gain == 1.0` — `whspr-config`'s `[capture] input_gain` default — is a
/// no-op for any already-normalized input: every sample is multiplied by
/// exactly `1.0` and then clamped, which never changes a value already in
/// range.
pub fn apply_gain(samples: &mut [f32], gain: f32) {
    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}
