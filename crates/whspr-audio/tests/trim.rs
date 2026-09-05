//! Silence-trimming behaviour for `whspr-audio`, exercised through the crate's
//! public API. These live as integration tests (rather than in a `#[cfg(test)]`
//! module inside `lib.rs`) to keep that file comfortably small; every symbol
//! under test is `pub`.

use tempfile::NamedTempFile;
use whspr_audio::{decode_wav, trim_silence_default};
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

#[test]
fn test_trim_silence_pause_in_middle_preserved() {
    // Build: [tone; 3200] + [silence; 1600] + [tone; 3200]
    // Mid-utterance pause should NOT be trimmed (only leading/trailing).
    let sample_rate = 16000u32;
    let freq = 440.0_f32;

    let mut samples = Vec::new();

    // First tone segment
    for i in 0..3200 {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        samples.push(sample);
    }

    // Mid-utterance pause (silence)
    samples.extend(vec![0.0; 1600]);

    // Second tone segment
    for i in 0..3200 {
        let t = (3200 + 1600 + i) as f32 / sample_rate as f32;
        let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        samples.push(sample);
    }

    let original_len = samples.len();
    let audio = AudioBuffer::new(samples, sample_rate);
    let trimmed = trim_silence_default(&audio);

    // Nothing should be trimmed (silence is in the middle, not at boundaries)
    // The trimmed buffer should be the same length as the original
    assert_eq!(
        trimmed.samples.len(),
        original_len,
        "middle silence should not be trimmed"
    );
}

#[test]
fn test_trim_silence_all_silence_degrades_gracefully() {
    // Build a buffer of all silence, longer than DEFAULT_MIN_KEEP_SAMPLES
    let sample_rate = 16000u32;
    let silence_samples = 6400; // Well over DEFAULT_MIN_KEEP_SAMPLES (1600)

    let samples = vec![0.0; silence_samples];
    let audio = AudioBuffer::new(samples, sample_rate);
    let trimmed = trim_silence_default(&audio);

    // Should NOT panic and should return the original buffer unchanged
    assert_eq!(
        trimmed.samples.len(),
        silence_samples,
        "all-silence buffer should not be trimmed"
    );
}

#[test]
fn test_trim_silence_too_short_buffer() {
    // A buffer shorter than one window should be returned unchanged
    let sample_rate = 16000u32;
    let short_len = 50; // Much shorter than a window

    let samples: Vec<f32> = (0..short_len).map(|i| (i as f32 * 0.01).sin()).collect();

    let audio = AudioBuffer::new(samples.clone(), sample_rate);
    let trimmed = trim_silence_default(&audio);

    // Should return unchanged
    assert_eq!(
        trimmed.samples.len(),
        short_len,
        "short buffer should not be trimmed"
    );
    for (i, &sample) in trimmed.samples.iter().enumerate() {
        assert!(
            (sample - audio.samples[i]).abs() < 1e-6,
            "short buffer should be unchanged"
        );
    }
}

#[test]
fn test_decode_wav_applies_silence_trimming() {
    // Write a WAV containing [silence; 1600] + [tone; 3200] + [silence; 1600] at 16kHz
    let temp_file = NamedTempFile::new().expect("failed to create temp file");
    let path = temp_file.path();

    let sample_rate = 16000u32;
    let freq = 440.0_f32;

    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(path, spec).expect("failed to create WAV writer");

        // Write leading silence (1600 samples = 0.1 seconds at 16kHz)
        for _ in 0..1600 {
            writer.write_sample(0i16).expect("failed to write sample");
        }

        // Write 440Hz tone at amplitude 0.5 (3200 samples = 0.2 seconds)
        for i in 0..3200 {
            let t = i as f32 / sample_rate as f32;
            let sample_f = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            let sample_i16 = (sample_f * 32767.0) as i16;
            writer
                .write_sample(sample_i16)
                .expect("failed to write sample");
        }

        // Write trailing silence (1600 samples)
        for _ in 0..1600 {
            writer.write_sample(0i16).expect("failed to write sample");
        }

        writer.finalize().expect("failed to finalize WAV");
    }

    // Decode the WAV
    let decoded = decode_wav(path).expect("failed to decode WAV");

    // The decoded buffer should have silence trimmed, leaving approximately 3200 samples
    // (we allow a small tolerance for floating-point rounding in the sine generation)
    assert!(
        decoded.samples.len() >= 3100 && decoded.samples.len() <= 3300,
        "decoded audio should be close to 3200 samples after trimming, got {}",
        decoded.samples.len()
    );
}
