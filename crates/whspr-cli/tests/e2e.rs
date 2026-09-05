//! Deterministic end-to-end tests for the whspr CLI. Verifies that the CLI
//! binary correctly parses arguments, handles files, and produces expected output.

use assert_cmd::Command;
use predicates::prelude::*;

/// Creates a minimal test WAV file with a given sample rate.
fn create_test_wav(path: &std::path::Path, sample_rate: u32, duration_secs: f32) -> anyhow::Result<()> {
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

#[test]
fn version_flag_exits_zero() {
    Command::cargo_bin("whspr")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn transcribe_nonexistent_file_fails_with_nonzero_exit() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "/nonexistent/file.wav"])
        .assert()
        .failure();
}

#[test]
fn transcribe_invalid_file_format_fails() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let invalid_file = temp_dir.path().join("not_a_wav.bin");
    std::fs::write(&invalid_file, b"not a WAV file").expect("failed to create invalid file");

    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", invalid_file.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn transcribe_batch_nonexistent_directory_fails() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe-batch", "/nonexistent/directory"])
        .assert()
        .failure();
}

#[test]
fn transcribe_with_json_flag_parses() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // This will fail at the ASR stage (whisper-local isn't available), but verifies
    // that the --json flag is parsed and doesn't cause a syntax error
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", fixture_path.to_str().unwrap(), "--json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("WhisperLocal not available")
            .or(predicate::str::contains("Error")));
}

#[test]
fn transcribe_help_mentions_asr_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--asr"));
}

#[test]
fn transcribe_help_mentions_refine_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--refine"));
}

#[test]
fn transcribe_help_mentions_json_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn transcribe_help_mentions_language_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--language"));
}

#[test]
fn transcribe_help_mentions_no_store_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--no-store"));
}
