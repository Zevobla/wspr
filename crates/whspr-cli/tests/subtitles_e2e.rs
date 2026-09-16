//! End-to-end tests for `whspr transcribe --format srt|vtt` (timecoded
//! subtitle export), split out of `e2e.rs` to keep that file under the
//! 600-line limit (AA-06).

use assert_cmd::Command;
use predicates::prelude::*;
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

mod common;
use common::{create_test_wav, isolated_config_dir, MOCK_TRANSCRIPT};

#[test]
fn transcribe_format_srt_prints_timecoded_cues() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
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

    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
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

    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
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

    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
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
    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
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
    let config_dir = isolated_config_dir();

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
            "--refine",
            "noop",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from deepgram mock"));
}
