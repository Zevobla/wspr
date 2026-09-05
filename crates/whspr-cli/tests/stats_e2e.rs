//! Deterministic end-to-end tests for the `whspr stats` subcommand.
//! Split out of `e2e.rs` (which covers `transcribe`/`transcribe-batch`) to
//! keep that file under this project's 600-line-per-file guideline, same
//! reasoning as `diarize_e2e.rs`.

use assert_cmd::Command;
use predicates::prelude::*;

/// Writes `lines` (already-serialized JSON objects, one per line) as
/// `history.jsonl` inside `dir` - the on-disk shape `main::save_to_history`
/// produces, seeded directly so these tests don't need to run a real
/// transcription first.
fn seed_history(dir: &std::path::Path, lines: &[&str]) {
    std::fs::write(dir.join("history.jsonl"), lines.join("\n") + "\n")
        .expect("failed to seed history.jsonl");
}

#[test]
fn stats_csv_prints_header_and_seeded_rows() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    seed_history(
        data_dir.path(),
        &[
            r#"{"text":"hello world","timestamp":1000,"asr":"mock","refine":"noop","source":"cli","wpm":120.0,"word_count":2}"#,
            r#"{"text":"goodbye","timestamp":2000,"asr":"mock","refine":"noop","source":"cli","wpm":90.0,"word_count":1}"#,
        ],
    );

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "stats",
            "--csv",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(concat!(
            "timestamp,asr,refine,wpm,word_count,text\n",
            "1000,mock,noop,120,2,hello world\n",
            "2000,mock,noop,90,1,goodbye\n",
        ));
}

#[test]
fn stats_csv_with_no_history_prints_only_header() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "stats",
            "--csv",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout("timestamp,asr,refine,wpm,word_count,text\n");
}

#[test]
fn stats_csv_quotes_text_containing_a_comma() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    seed_history(
        data_dir.path(),
        &[
            r#"{"text":"hello, world","timestamp":1000,"asr":"mock","refine":"noop","source":"cli","wpm":120.0,"word_count":2}"#,
        ],
    );

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "stats",
            "--csv",
            "--data-dir",
            data_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"hello, world\""));
}

#[test]
fn stats_table_mode_prints_wpm_and_words() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    seed_history(
        data_dir.path(),
        &[
            r#"{"text":"hello world","timestamp":1000,"asr":"mock","refine":"noop","source":"cli","wpm":120.0,"word_count":2}"#,
        ],
    );

    Command::cargo_bin("whspr")
        .unwrap()
        .args(["stats", "--data-dir", data_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("wpm=120"))
        .stdout(predicate::str::contains("words=2"));
}

#[test]
fn stats_help_mentions_csv_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["stats", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--csv"));
}
