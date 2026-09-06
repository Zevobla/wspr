use whspr_audio::resample_to_16k_mono;
use whspr_core::AudioBuffer;

#[test]
fn test_resample_to_16k_mono_from_22050hz() {
    // Test downsampling from 22050 Hz to 16000 Hz
    let input_samples = 2205; // 0.1 seconds at 22050 Hz
    let buffer = AudioBuffer::new(vec![0.5; input_samples], 22050);
    let resampled = resample_to_16k_mono(&buffer).expect("resampling failed");

    assert_eq!(resampled.sample_rate, 16000);
    // Expected output length at 16kHz: (2205 * 16000 / 22050) ≈ 1600
    let expected_len = (input_samples as f64 * 16000.0 / 22050.0) as usize;
    assert!(
        (resampled.samples.len() as i32 - expected_len as i32).abs() <= 2,
        "resampled length should be ~{}, got {}",
        expected_len,
        resampled.samples.len()
    );
}

#[test]
fn test_resample_to_16k_mono_from_44100hz() {
    // Test downsampling from 44100 Hz to 16000 Hz
    let input_samples = 4410; // 0.1 seconds at 44100 Hz
    let buffer = AudioBuffer::new(vec![0.5; input_samples], 44100);
    let resampled = resample_to_16k_mono(&buffer).expect("resampling failed");

    assert_eq!(resampled.sample_rate, 16000);
    // Expected output length at 16kHz: (4410 * 16000 / 44100) ≈ 1600
    let expected_len = (input_samples as f64 * 16000.0 / 44100.0) as usize;
    assert!(
        (resampled.samples.len() as i32 - expected_len as i32).abs() <= 2,
        "resampled length should be ~{}, got {}",
        expected_len,
        resampled.samples.len()
    );
}
