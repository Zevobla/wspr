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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_gain_of_one_is_a_no_op_for_in_range_samples() {
        let mut samples = vec![-0.5, 0.0, 0.25, 0.9];
        let original = samples.clone();
        apply_gain(&mut samples, 1.0);
        assert_eq!(samples, original);
    }

    #[test]
    fn apply_gain_scales_samples() {
        let mut samples = vec![0.1, -0.2, 0.3];
        apply_gain(&mut samples, 2.0);
        assert!((samples[0] - 0.2).abs() < 1e-6);
        assert!((samples[1] - (-0.4)).abs() < 1e-6);
        assert!((samples[2] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn apply_gain_clamps_to_valid_range() {
        let mut samples = vec![0.9, -0.9];
        apply_gain(&mut samples, 3.0);
        assert_eq!(samples, vec![1.0, -1.0]);
    }

    #[test]
    fn apply_gain_on_empty_slice_does_not_panic() {
        let mut samples: Vec<f32> = Vec::new();
        apply_gain(&mut samples, 2.0);
        assert!(samples.is_empty());
    }
}
