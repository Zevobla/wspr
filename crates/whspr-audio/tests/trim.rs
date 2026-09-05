//! Silence-trimming behaviour for `whspr-audio`, exercised through the crate's
//! public API. These live as integration tests (rather than in a `#[cfg(test)]`
//! module inside `lib.rs`) to keep that file comfortably small; every symbol
//! under test is `pub`.

use whspr_audio::trim_silence_default;
use whspr_core::AudioBuffer;

#[test]
fn test_trim_silence_leading_trailing_removed() {
    // Build: [silence; 1600] + [tone; 3200] + [silence; 1600] at 16kHz
    let sample_rate = 16000u32;

    let mut samples = Vec::new();

    // Leading silence: 1600 samples at 0.0
    samples.extend(vec![0.0; 1600]);

    // Tone: 3200 samples of 440Hz sine at amplitude 0.5
    let freq = 440.0_f32;
    for i in 0..3200 {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        samples.push(sample);
    }

    // Trailing silence: 1600 samples at 0.0
    samples.extend(vec![0.0; 1600]);

    let audio = AudioBuffer::new(samples.clone(), sample_rate);
    let trimmed = trim_silence_default(&audio);

    // Should have trimmed the leading and trailing silence
    assert_eq!(
        trimmed.samples.len(),
        3200,
        "trimmed audio should be exactly 3200 samples"
    );

    // The trimmed samples should match the original tone segment
    for (i, &sample) in trimmed.samples.iter().enumerate() {
        let expected = samples[1600 + i];
        assert!((sample - expected).abs() < 1e-6, "sample {} mismatch", i);
    }
}
