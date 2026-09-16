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

/// High-pass cutoff frequency, in Hz, used by `suppress_noise`'s first
/// stage. ~80 Hz sits below the fundamental of essentially all speech
/// while still removing DC offset, mic-handling rumble, and HVAC hum.
const HIGH_PASS_CUTOFF_HZ: f32 = 80.0;

/// Width, in milliseconds, of the window `suppress_noise` scans for the
/// quietest stretch of audio, whose RMS becomes the estimated noise floor.
const NOISE_FLOOR_WINDOW_MS: u32 = 100;

/// Width, in milliseconds, of the frames the noise gate evaluates and
/// attenuates independently. Short enough that the attack/release ramp
/// between adjacent frames (see `noise_gate_in_place`) stays imperceptible.
const GATE_FRAME_MS: u32 = 10;

/// A frame is gated (treated as noise) when its RMS is at or below the
/// estimated noise floor times this multiplier. `2.0` gives a couple of dB
/// of headroom above the floor so genuine low-level noise gets caught
/// without also catching quiet speech.
const GATE_THRESHOLD_MULT: f32 = 2.0;

/// How far a gated frame's samples are scaled down by — not to zero, so a
/// gated stretch reads as "quieter" rather than an abrupt, ear-catching
/// mute.
const GATE_ATTENUATION: f32 = 0.15;

/// The noise gate only engages when the loudest 100ms window's RMS is at
/// least this many times the quietest window's RMS — roughly 18dB
/// (`20 * log10(8.0) ≈ 18.06dB`). Below this ratio, the clip has no
/// clearly-quieter "noise" stretch distinct from its "signal" (e.g. a
/// push-to-talk recording where the user talks the whole time, so even
/// its quietest moment is still speech): gating would just clip the
/// recording's own quiet syllables rather than remove background noise,
/// so the gate stays off for the whole clip.
const MIN_GATE_DYNAMIC_RANGE: f32 = 8.0;

/// The noise gate only engages when the estimated floor is also quiet in
/// absolute terms — at or below this RMS. Guards against engaging on a
/// clip that has *some* dynamic range but is never actually quiet (e.g.
/// consistently-present quieter background music/speech rather than
/// silence or genuine noise floor).
const MAX_NOISE_FLOOR_RMS: f32 = 0.05;

/// An honest, minimal noise-reduction chain, run in place on `samples`
/// (interpreted as mono `f32` at `sample_rate` Hz):
///
/// 1. **High-pass** — a one-pole IIR high-pass filter (~80 Hz cutoff,
///    `HIGH_PASS_CUTOFF_HZ`) removes DC offset and very-low-frequency
///    rumble that sits below the range of human speech.
/// 2. **Noise gate** — the RMS of the quietest and loudest contiguous
///    100ms windows in the (high-passed) signal are found. The gate only
///    engages at all if there's a clear, quiet noise floor to gate — see
///    `MIN_GATE_DYNAMIC_RANGE`/`MAX_NOISE_FLOOR_RMS` — which rules out
///    e.g. a push-to-talk clip with no silence in it. When it does
///    engage, the signal is processed in ~10ms frames; any frame whose
///    RMS is at or below `floor * GATE_THRESHOLD_MULT` is attenuated
///    toward (not to) silence by `GATE_ATTENUATION`, with the per-sample
///    gain linearly ramped from the previous frame's gain to the new
///    frame's target across each frame — a short attack/release so a
///    speech onset right after a quiet stretch isn't clipped by a hard
///    on/off transition.
///
/// What this is **not**: there's no spectral subtraction (no FFT
/// involved at all), no noise-profile learning across calls, and no ML
/// model. It's two well-understood, cheap DSP stages applied once — good
/// for steady background hiss/hum/rumble ahead of or around speech, not
/// for suppressing e.g. a second talker or transient noises.
///
/// A silent or empty `samples`, or `sample_rate == 0`, is left/returns
/// unchanged rather than dividing by zero.
pub fn suppress_noise(samples: &mut [f32], sample_rate: u32) {
    if samples.is_empty() || sample_rate == 0 {
        return;
    }
    high_pass_in_place(samples, sample_rate, HIGH_PASS_CUTOFF_HZ);
    noise_gate_in_place(samples, sample_rate);
}

/// One-pole IIR high-pass filter, applied in place. Standard textbook
/// form: `y[i] = alpha * (y[i-1] + x[i] - x[i-1])`, with `alpha` derived
/// from the cutoff frequency and sample rate.
fn high_pass_in_place(samples: &mut [f32], sample_rate: u32, cutoff_hz: f32) {
    let dt = 1.0 / sample_rate as f32;
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let alpha = rc / (rc + dt);

    let mut prev_x = samples[0];
    let mut prev_y = samples[0];
    for sample in samples.iter_mut() {
        let x = *sample;
        let y = alpha * (prev_y + x - prev_x);
        *sample = y;
        prev_x = x;
        prev_y = y;
    }
}

/// RMS energy of `samples`; `0.0` for an empty slice.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Scans non-overlapping `NOISE_FLOOR_WINDOW_MS` windows across `samples`
/// and returns `(quietest, loudest)` RMS among them. For a `samples`
/// shorter than one window, both are that whole slice's RMS (there's
/// nothing longer to compare it against).
fn window_rms_range(samples: &[f32], sample_rate: u32) -> (f32, f32) {
    let window_len = ((sample_rate as u64 * NOISE_FLOOR_WINDOW_MS as u64) / 1000).max(1) as usize;
    if samples.len() <= window_len {
        let r = rms(samples);
        return (r, r);
    }

    let mut min_rms = f32::MAX;
    let mut max_rms = 0.0f32;
    let mut start = 0;
    while start + window_len <= samples.len() {
        let window_rms = rms(&samples[start..start + window_len]);
        min_rms = min_rms.min(window_rms);
        max_rms = max_rms.max(window_rms);
        start += window_len;
    }
    (min_rms, max_rms)
}

/// Attenuates frames of `samples` below the estimated noise floor,
/// ramping the applied gain linearly across each frame so the transition
/// in/out of a gated stretch is gradual rather than a hard on/off click.
///
/// Does nothing if the clip doesn't have a clear, quiet noise floor to
/// gate in the first place — see `MIN_GATE_DYNAMIC_RANGE`/
/// `MAX_NOISE_FLOOR_RMS` — so a speech-only clip with no genuine silence
/// (e.g. push-to-talk) isn't gated on its own quiet syllables.
fn noise_gate_in_place(samples: &mut [f32], sample_rate: u32) {
    let (floor, loudest) = window_rms_range(samples, sample_rate);

    let has_dynamic_range = loudest >= floor * MIN_GATE_DYNAMIC_RANGE;
    let floor_is_quiet_enough = floor <= MAX_NOISE_FLOOR_RMS;
    if !has_dynamic_range || !floor_is_quiet_enough {
        return;
    }

    let threshold = floor * GATE_THRESHOLD_MULT;
    let frame_len = ((sample_rate as u64 * GATE_FRAME_MS as u64) / 1000).max(1) as usize;

    let mut current_gain = 1.0f32;
    let mut start = 0;
    while start < samples.len() {
        let end = (start + frame_len).min(samples.len());
        let frame = &mut samples[start..end];
        let frame_rms = rms(frame);
        let target_gain = if frame_rms <= threshold {
            GATE_ATTENUATION
        } else {
            1.0
        };

        let frame_len_actual = frame.len();
        for (i, s) in frame.iter_mut().enumerate() {
            let t = if frame_len_actual > 1 {
                i as f32 / (frame_len_actual - 1) as f32
            } else {
                1.0
            };
            let gain = current_gain + (target_gain - current_gain) * t;
            *s *= gain;
        }

        current_gain = target_gain;
        start = end;
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

    #[test]
    fn suppress_noise_on_empty_slice_does_not_panic() {
        let mut samples: Vec<f32> = Vec::new();
        suppress_noise(&mut samples, 16000);
        assert!(samples.is_empty());
    }

    #[test]
    fn suppress_noise_with_zero_sample_rate_does_not_panic_or_divide_by_zero() {
        let mut samples = vec![0.1, 0.2, 0.3];
        let original = samples.clone();
        suppress_noise(&mut samples, 0);
        // sample_rate == 0 is nonsensical input; suppress_noise leaves it
        // untouched rather than dividing by zero building the filter.
        assert_eq!(samples, original);
    }

    #[test]
    fn suppress_noise_removes_dc_offset() {
        let sample_rate = 16000u32;
        // A constant (DC) signal, no speech content at all.
        let mut samples = vec![0.3f32; sample_rate as usize];
        suppress_noise(&mut samples, sample_rate);

        // The high-pass filter converges toward 0 for a constant input;
        // check the back half (past the filter's settling time) has a
        // mean and per-sample magnitude close to 0.
        let tail = &samples[sample_rate as usize / 2..];
        let mean: f32 = tail.iter().sum::<f32>() / tail.len() as f32;
        assert!(mean.abs() < 0.01, "DC offset not removed: mean={mean}");
    }

    #[test]
    fn suppress_noise_keeps_silence_quiet() {
        let sample_rate = 16000u32;
        // Low-level noise, not exact silence (RMS 0 would make the gate's
        // threshold 0 too, trivially "passing" - a more realistic quiet
        // room has some noise floor).
        let mut samples: Vec<f32> = (0..sample_rate)
            .map(|i| 0.001 * ((i as f32) * 0.7).sin())
            .collect();
        suppress_noise(&mut samples, sample_rate);

        let output_rms = rms(&samples);
        assert!(
            output_rms < 0.005,
            "near-silence should stay quiet: output RMS={output_rms}"
        );
    }

    #[test]
    fn suppress_noise_preserves_tone_rms_after_a_quiet_lead_in() {
        let sample_rate = 16000u32;
        // 150ms of near-silence, then a 1kHz tone - a realistic shape
        // (quiet room tone, then speech/tone) that gives the noise-floor
        // estimator a genuinely quiet window to measure, distinct from
        // the loud part that follows.
        let quiet_len = (sample_rate as usize * 150) / 1000;
        let tone_len = sample_rate as usize; // 1 second of tone
        let mut samples = Vec::with_capacity(quiet_len + tone_len);
        samples.extend(std::iter::repeat_n(0.0f32, quiet_len));
        let freq = 1000.0f32;
        for i in 0..tone_len {
            let t = i as f32 / sample_rate as f32;
            samples.push(0.5 * (2.0 * std::f32::consts::PI * freq * t).sin());
        }

        let original_tone_rms = rms(&samples[quiet_len..]);

        suppress_noise(&mut samples, sample_rate);

        // Skip a short settling window right at the quiet->tone boundary
        // (the gate's attack ramp), then compare the steady-state tone.
        let settle = (sample_rate as usize * 20) / 1000;
        let steady_tone = &samples[quiet_len + settle..];
        let output_tone_rms = rms(steady_tone);

        let ratio = output_tone_rms / original_tone_rms;
        assert!(
            (0.8..=1.05).contains(&ratio),
            "tone RMS should survive suppress_noise within tolerance: \
             original={original_tone_rms}, output={output_tone_rms}, ratio={ratio}"
        );
    }
}
