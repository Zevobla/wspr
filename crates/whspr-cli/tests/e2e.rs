//! Deterministic end-to-end tests for the whspr CLI. Verifies that the CLI
//! binary correctly parses arguments, handles files, and produces expected output.
//! (`whspr diarize` has its own suite in `diarize_e2e.rs`.)

use assert_cmd::Command;
use predicates::prelude::*;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

mod common;
use common::create_test_wav;

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

    // --asr mock keeps this deterministic and offline (see build_asr_backend
    // in main.rs; the no-flag default now builds a real WhisperLocal
    // backend); this test's job is just verifying --json is parsed without
    // a syntax error.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
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
fn transcribe_with_asr_mock_prints_mock_transcript() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // The CLI's no-flag default now builds a real WhisperLocal backend (see
    // build_asr_backend in main.rs), so `--asr mock` is this suite's
    // explicit, deterministic, offline opt-in instead.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(MOCK_TRANSCRIPT));
}

#[test]
fn transcribe_normalizes_numbers_on_the_real_path() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // --asr-mock-text (hidden, test-only) drives a normalizable phrase
    // through --asr mock, proving build_refiner's NormalizingRefiner
    // wrapping (F-10/F-11/F-12) actually runs on the real `transcribe`
    // path rather than sitting dead behind its own unit tests.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--asr-mock-text",
            "bring twenty five copies",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("25 copies"));
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
            "--asr",
            "mock",
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
            "--asr",
            "mock",
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
            "--asr",
            "mock",
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
            "--asr",
            "mock",
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
fn transcribe_format_srt_prints_timecoded_cues() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // MockAsr's canned Transcript never populates per-segment timing, so
    // this exercises `subtitles`'s degenerate single-cue fallback rather
    // than real segment-per-line output - see subtitles.rs's own unit
    // tests for the multi-segment case.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--format",
            "srt",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("1\n00:00:00,000 --> "))
        .stdout(predicate::str::contains(MOCK_TRANSCRIPT));
}

#[test]
fn transcribe_format_vtt_prints_webvtt_header() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--format",
            "vtt",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("WEBVTT\n\n1\n"))
        .stdout(predicate::str::contains(MOCK_TRANSCRIPT));
}

#[test]
fn transcribe_format_unknown_value_fails_with_clear_error() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--format",
            "docx",
            "--no-store",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown export format"));
}

#[test]
fn transcribe_format_takes_precedence_over_json() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--format",
            "srt",
            "--json",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("1\n00:00:00,000 --> "));
}

// The wiremock server's background listener task and the subprocess spawned
// by assert_cmd both need to make progress at once: the subprocess call
// blocks the calling OS thread synchronously, so a single-threaded runtime
// would never get to poll the mock server. `flavor = "multi_thread"` puts
// them on separate worker threads.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transcribe_with_asr_openai_succeeds_against_mock_server() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/audio/transcriptions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"text": "hello from openai mock"})),
        )
        .mount(&mock_server)
        .await;

    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    // --asr-base-url and --asr-api-key are hidden, test-only overrides (see
    // build_asr_backend in main.rs) that let a real cloud backend be
    // exercised end-to-end against a local mock server instead of the
    // network.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "openai",
            "--asr-base-url",
            &mock_server.uri(),
            "--asr-api-key",
            "test-key",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from openai mock"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transcribe_with_asr_deepgram_succeeds_against_mock_server() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/listen"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": {
                "channels": [
                    {"alternatives": [{"transcript": "hello from deepgram mock"}]}
                ]
            }
        })))
        .mount(&mock_server)
        .await;

    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "deepgram",
            "--asr-base-url",
            &mock_server.uri(),
            "--asr-api-key",
            "test-key",
            "--no-store",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from deepgram mock"));
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

#[test]
fn transcribe_help_mentions_format_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"));
}
