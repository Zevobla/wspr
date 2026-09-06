//! Shared helpers for the CLI's e2e test binaries. Deliberately at
//! `tests/common/mod.rs` (the old-style module path) rather than
//! `tests/common.rs`: Cargo only auto-discovers direct `tests/*.rs` files as
//! their own test binaries, so this naming keeps `common` a plain shared
//! module instead of an (empty, pointless) test binary of its own.

/// Creates a minimal test WAV file with a given sample rate.
pub fn create_test_wav(
    path: &std::path::Path,
    sample_rate: u32,
    duration_secs: f32,
) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;
    let sample_count = (sample_rate as f32 * duration_secs) as usize;

    // Write silent samples
    for _ in 0..sample_count {
        writer.write_sample(0i16)?;
    }

    writer.finalize()?;
    Ok(())
}

/// Creates a test WAV of a known duration whose samples are loud enough
/// (well above `whspr_audio::DEFAULT_SILENCE_THRESHOLD`) that
/// `trim_silence` leaves it untouched - so the decoded `AudioBuffer`'s
/// `duration_secs()` comes out exactly equal to `duration_secs`, letting a
/// test predict the resulting wpm precisely. `create_test_wav`'s all-silent
/// samples can't be used for that: they'd get trimmed to whatever the
/// silence-trim floor is, not a value the test controls.
///
/// `#[allow(dead_code)]`: each `tests/*.rs` file that does `mod common;`
/// compiles this module as its own crate, and only `e2e.rs` calls this
/// one. `diarize_e2e.rs` shares the module for `create_test_wav` alone,
/// so without the allow its copy would fail `-D warnings` on unused code.
#[allow(dead_code)]
pub fn create_test_wav_with_tone(
    path: &std::path::Path,
    sample_rate: u32,
    duration_secs: f32,
) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;
    let sample_count = (sample_rate as f32 * duration_secs) as usize;

    // A square wave at a fixed amplitude well above the silence threshold.
    for i in 0..sample_count {
        let sample: i16 = if i % 2 == 0 { 5000 } else { -5000 };
        writer.write_sample(sample)?;
    }

    writer.finalize()?;
    Ok(())
}
