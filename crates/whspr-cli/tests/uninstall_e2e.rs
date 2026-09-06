//! Deterministic end-to-end tests for the `whspr uninstall` subcommand
//! (AH-08). Split out of `e2e.rs` to keep that file under this project's
//! 600-line-per-file guideline, same reasoning as `diarize_e2e.rs`.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn uninstall_without_yes_is_a_dry_run() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    std::fs::write(data_dir.path().join("history.jsonl"), "{}\n")
        .expect("failed to seed a file to prove it survives the dry run");

    Command::cargo_bin("whspr")
        .unwrap()
        .args(["uninstall", "--data-dir", data_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("--yes"));

    assert!(
        data_dir.path().exists(),
        "a dry run must not delete anything"
    );
}

#[test]
fn uninstall_with_yes_removes_the_data_dir() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    let dir_path = data_dir.path().to_path_buf();
    std::fs::write(dir_path.join("history.jsonl"), "{}\n").expect("failed to seed history file");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "uninstall",
            "--yes",
            "--data-dir",
            dir_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    assert!(
        !dir_path.exists(),
        "--yes should actually remove the (overridden) config/data directory"
    );
}

#[test]
fn uninstall_with_yes_on_a_missing_dir_is_not_an_error() {
    let data_dir = tempfile::tempdir().expect("failed to create data dir");
    let nonexistent = data_dir.path().join("never-created");

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "uninstall",
            "--yes",
            "--data-dir",
            nonexistent.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No"));
}

#[test]
fn uninstall_help_mentions_yes_flag() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--yes"));
}
