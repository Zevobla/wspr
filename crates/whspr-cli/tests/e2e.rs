//! Deterministic end-to-end tests for the whspr CLI. Verifies that the CLI
//! binary correctly parses arguments, handles files, and produces expected output.

use assert_cmd::Command;
use predicates::prelude::*;

/// Creates a minimal test WAV file with a given sample rate.
fn create_test_wav(
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

    // With no --asr flag, the default backend is MockAsr (see build_asr_backend),
    // so this now succeeds; verifies --json is parsed without a syntax error.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--json",
            "--no-store",
        ])
        .assert()
        .success();
}

/// The canned transcript text MockAsr::default() always returns
/// (whspr_core::testkit::MockAsr).
const MOCK_TRANSCRIPT: &str = "the quick brown fox jumps over the lazy dog";

#[test]
fn transcribe_default_no_flags_prints_mock_transcript() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // No --asr flag at all: this is the CLI's most basic, documented use
    // case (see root CLAUDE.md's "Build & test" section) and must succeed
    // offline against the default MockAsr backend.
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", fixture_path.to_str().unwrap(), "--no-store"])
        .assert()
        .success()
        .stdout(predicate::str::contains(MOCK_TRANSCRIPT));
}

#[test]
fn transcribe_json_output_has_expected_fields() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let output = Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--json",
            "--no-store",
        ])
        .output()
        .expect("failed to run whspr");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout was not valid UTF-8");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout was not valid JSON");

    assert_eq!(
        parsed.get("text").and_then(|v| v.as_str()),
        Some(MOCK_TRANSCRIPT)
    );
    assert_eq!(parsed.get("asr").and_then(|v| v.as_str()), Some("mock"));
    assert_eq!(parsed.get("refine").and_then(|v| v.as_str()), Some("noop"));
    assert!(
        parsed.get("wpm").and_then(|v| v.as_f64()).is_some(),
        "expected a numeric wpm field, got {:?}",
        parsed.get("wpm")
    );
}

#[test]
fn transcribe_batch_succeeds_with_one_result_per_file() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    create_test_wav(&temp_dir.path().join("a.wav"), 16000, 0.1).expect("failed to create a.wav");
    create_test_wav(&temp_dir.path().join("b.wav"), 16000, 0.1).expect("failed to create b.wav");

    let output = Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe-batch",
            temp_dir.path().to_str().unwrap(),
            "--json",
            "--no-store",
        ])
        .output()
        .expect("failed to run whspr");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout was not valid UTF-8");
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one JSON result per input file, got: {:?}",
        lines
    );

    for line in lines {
        let parsed: serde_json::Value =
            serde_json::from_str(line).expect("each output line should be valid JSON");
        assert_eq!(
            parsed.get("text").and_then(|v| v.as_str()),
            Some(MOCK_TRANSCRIPT)
        );
        assert!(parsed.get("file").and_then(|v| v.as_str()).is_some());
    }
}

#[test]
fn transcribe_appends_history_entry_when_stored() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // --data-dir is a hidden, test-only override (see resolve_data_dir in
    // main.rs) that redirects history writes away from the real platform
    // data directory.
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    let history_path = data_dir.path().join("history.jsonl");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let contents = std::fs::read_to_string(&history_path).expect("history.jsonl should exist");
    let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected exactly one history line");

    let entry: serde_json::Value =
        serde_json::from_str(lines[0]).expect("history line should be valid JSON");
    assert_eq!(
        entry.get("text").and_then(|v| v.as_str()),
        Some(MOCK_TRANSCRIPT)
    );
    assert_eq!(entry.get("asr").and_then(|v| v.as_str()), Some("mock"));
    assert_eq!(entry.get("refine").and_then(|v| v.as_str()), Some("noop"));
    assert_eq!(entry.get("source").and_then(|v| v.as_str()), Some("cli"));
    assert!(entry.get("timestamp").and_then(|v| v.as_u64()).is_some());
    assert!(entry.get("wpm").is_some());
    assert_eq!(
        entry.get("word_count").and_then(|v| v.as_u64()),
        Some(MOCK_TRANSCRIPT.split_whitespace().count() as u64)
    );
}

#[test]
fn transcribe_no_store_skips_history_entry() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    let history_path = data_dir.path().join("history.jsonl");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--no-store",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(
        !history_path.exists(),
        "--no-store must not create a history file"
    );
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
