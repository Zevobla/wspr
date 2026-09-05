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
