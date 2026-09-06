//! Deterministic end-to-end tests for the `whspr diarize` subcommand.
//! Split out of `e2e.rs` to keep that file under this project's
//! 600-line-per-file guideline -- see `tests/common/mod.rs` for the shared
//! `create_test_wav` fixture helper both files use.

use assert_cmd::Command;

mod common;
use common::create_test_wav;

#[test]
fn diarize_with_mock_backend_prints_speaker_labeled_turns() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");

    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    let output = Command::cargo_bin("whspr")
        .unwrap()
        // SPEAKER_MODEL_DIR (see SherpaDiarizer::resolve_model_dir) is a
        // bring-your-own-model env var a developer's own shell might have
        // set; clear it so this test exercises the MockDiarizer fallback
        // it's named for, regardless of what environment `cargo test`
        // happens to run in.
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
