//! Deterministic end-to-end tests for the whspr CLI. Verifies that the CLI
//! binary correctly parses arguments, handles files, and produces expected output.

use assert_cmd::Command;
use predicates::prelude::*;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

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
fn diarize_with_mock_backend_prints_speaker_labeled_turns() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    let output = Command::cargo_bin("whspr")
        .unwrap()
        // The dev shell sets SPEAKER_MODEL_DIR to a real, pinned model
        // directory (see SherpaDiarizer::resolve_model_dir) -- clear it so
        // this test exercises the MockDiarizer fallback it's named for,
        // regardless of what environment `cargo test` happens to run in.
        .env_remove("SPEAKER_MODEL_DIR")
        .args([
            "diarize",
            fixture_path.to_str().unwrap(),
            "--json",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run whspr diarize");

    assert!(output.status.success(), "diarize should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout was not valid UTF-8");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(stdout.trim()).expect("stdout was not valid JSON array");

    assert_eq!(
        parsed.len(),
        2,
        "MockDiarizer should return exactly 2 turns"
    );
    assert_eq!(
        parsed[0].get("speaker").and_then(|v| v.as_str()),
        Some("Speaker 1"),
        "first turn should be labeled Speaker 1"
    );
    assert_eq!(
        parsed[1].get("speaker").and_then(|v| v.as_str()),
        Some("Speaker 2"),
        "second turn should be labeled Speaker 2"
    );
}

#[test]
fn diarize_persists_speaker_matches_across_runs() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path_1 = temp_dir.path().join("test1.wav");
    let fixture_path_2 = temp_dir.path().join("test2.wav");
    create_test_wav(&fixture_path_1, 16000, 0.1).expect("failed to create test WAV 1");
    create_test_wav(&fixture_path_2, 16000, 0.1).expect("failed to create test WAV 2");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    let speakers_path = data_dir.path().join("speakers.json");

    // First run (SPEAKER_MODEL_DIR cleared -- see the mock-backend test
    // above for why -- both runs need the deterministic MockDiarizer).
    let output_1 = Command::cargo_bin("whspr")
        .unwrap()
        .env_remove("SPEAKER_MODEL_DIR")
        .args([
            "diarize",
            fixture_path_1.to_str().unwrap(),
            "--json",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run whspr diarize (run 1)");

    assert!(
        output_1.status.success(),
        "first diarize run should succeed"
    );

    let stdout_1 = String::from_utf8(output_1.stdout).expect("stdout 1 was not valid UTF-8");
    let parsed_1: Vec<serde_json::Value> =
        serde_json::from_str(stdout_1.trim()).expect("stdout 1 was not valid JSON array");

    let speaker_1_first = parsed_1[0]
        .get("speaker")
        .and_then(|v| v.as_str())
        .expect("first turn of run 1 should have speaker");
    let speaker_2_first = parsed_1[1]
        .get("speaker")
        .and_then(|v| v.as_str())
        .expect("second turn of run 1 should have speaker");

    // Second run against a different file but same data_dir
    let output_2 = Command::cargo_bin("whspr")
        .unwrap()
        .env_remove("SPEAKER_MODEL_DIR")
        .args([
            "diarize",
            fixture_path_2.to_str().unwrap(),
            "--json",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run whspr diarize (run 2)");

    assert!(
        output_2.status.success(),
        "second diarize run should succeed"
    );

    let stdout_2 = String::from_utf8(output_2.stdout).expect("stdout 2 was not valid UTF-8");
    let parsed_2: Vec<serde_json::Value> =
        serde_json::from_str(stdout_2.trim()).expect("stdout 2 was not valid JSON array");

    let speaker_1_second = parsed_2[0]
        .get("speaker")
        .and_then(|v| v.as_str())
        .expect("first turn of run 2 should have speaker");
    let speaker_2_second = parsed_2[1]
        .get("speaker")
        .and_then(|v| v.as_str())
        .expect("second turn of run 2 should have speaker");

    // Both runs should assign the same speaker IDs (proving persistence)
    assert_eq!(
        speaker_1_first, speaker_1_second,
        "first speaker should match across runs"
    );
    assert_eq!(
        speaker_2_first, speaker_2_second,
        "second speaker should match across runs"
    );

    assert!(
        speakers_path.exists(),
        "speakers.json should exist after runs"
    );
}

#[test]
fn diarize_with_nonexistent_model_dir_fails_with_clear_error() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "diarize",
            fixture_path.to_str().unwrap(),
            "--model-dir",
            "/nonexistent/path",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn diarize_falls_back_to_speaker_model_dir_env_var() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    // No --model-dir flag and no config file, but SPEAKER_MODEL_DIR is set
    // to a bogus path: this should still attempt a real SherpaDiarizer
    // (and fail on that nonexistent directory) rather than silently
    // falling back to MockDiarizer, proving build_diarizer actually
    // consults the env var (see SherpaDiarizer::resolve_model_dir).
    let output = Command::cargo_bin("whspr")
        .unwrap()
        .env("SPEAKER_MODEL_DIR", "/nonexistent/from-env")
        .args([
            "diarize",
            fixture_path.to_str().unwrap(),
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run whspr diarize");

    assert!(
        !output.status.success(),
        "should fail since /nonexistent/from-env doesn't exist"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr was not valid UTF-8");
    assert!(
        stderr.contains("from-env"),
        "expected the error to mention the SPEAKER_MODEL_DIR-sourced path, got: {stderr}"
    );
}

#[test]
fn diarize_with_unknown_embedding_choice_fails_with_clear_error() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    // --embedding is only consulted once --model-dir opts into a real
    // backend (mirrors --asr's "explicit opt-in" philosophy), so this
    // exercises SpeakerEmbeddingChoice::from_str's error path without
    // needing any real model files -- it fails on the unknown choice
    // before ever touching the (also-nonexistent) model_dir.
    let output = Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "diarize",
            fixture_path.to_str().unwrap(),
            "--model-dir",
            "/nonexistent/path",
            "--embedding",
            "not-a-real-embedding-choice",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run whspr diarize");

    assert!(
        !output.status.success(),
        "should fail on unknown --embedding"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr was not valid UTF-8");
    assert!(
        stderr.contains("embedding"),
        "expected the error to mention the embedding choice, got: {stderr}"
    );
}
