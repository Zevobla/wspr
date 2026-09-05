//! Deterministic end-to-end seed: exercises the real built binary against
//! the mock pipeline (no real audio, no network, no model files) so later
//! teams have a working harness to extend once real backends land.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_flag_exits_zero() {
    Command::cargo_bin("whspr")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn transcribe_prints_mock_transcript() {
    Command::cargo_bin("whspr")
        .unwrap()
        .args(["transcribe", "/dev/null"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "the quick brown fox jumps over the lazy dog",
        ));
}
