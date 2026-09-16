//! Deterministic end-to-end tests for the J-11 shorten toggle
//! (`[capture].shorten` / `whspr transcribe --shorten`). Split out of
//! `e2e.rs` (new file, rather than growing it) to keep that file under
//! this project's 600-line-per-file guideline -- see `tests/common/mod.rs`
//! for the shared `create_test_wav`/`isolated_config_dir` helpers both
//! files use.
//!
//! Every test here drives `--asr mock --asr-mock-text "..." --refine noop`
//! with a phrase containing "sort of"/"kind of" rather than "um"/"uh":
//! those two phrases are only ever removed by the new shorten pass, unlike
//! plain "um"/"uh", which whspr-refine's separate, always-on filler pass
//! already strips regardless of this toggle -- see
//! `whspr_refine::normalize::shorten`'s module doc for that overlap.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

mod common;
use common::{create_test_wav, isolated_config_dir};

/// Writes `[capture]\nshorten = <value>` into `config_dir`'s config.toml.
fn write_shorten_config(config_dir: &std::path::Path, value: bool) {
    let config_path = config_dir.join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
    writeln!(file, "[capture]").expect("failed to write capture header");
    writeln!(file, "shorten = {value}").expect("failed to write shorten");
}

#[test]
fn transcribe_config_shorten_true_removes_filler_phrase() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");
    let config_dir = isolated_config_dir();
    write_shorten_config(config_dir.path(), true);

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--asr-mock-text",
            "it's sort of ready",
            "--refine",
            "noop",
            "--no-store",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("it's ready"))
        .stdout(predicate::str::contains("sort of").not());
}

#[test]
fn transcribe_config_shorten_false_leaves_filler_phrase_alone() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");
    let config_dir = isolated_config_dir();
    write_shorten_config(config_dir.path(), false);

    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--asr-mock-text",
            "it's sort of ready",
            "--refine",
            "noop",
            "--no-store",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("it's sort of ready"));
}

#[test]
fn transcribe_shorten_flag_overrides_config_off() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");
    let config_dir = isolated_config_dir();
    write_shorten_config(config_dir.path(), false);

    // Config says off, but --shorten true should win, same as --asr wins
    // over config.asr.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--asr-mock-text",
            "it's kind of tired",
            "--refine",
            "noop",
            "--shorten",
            "true",
            "--no-store",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("it's tired"))
        .stdout(predicate::str::contains("kind of").not());
}

#[test]
fn transcribe_shorten_flag_overrides_config_on() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let fixture_path = temp_dir.path().join("test.wav");
    create_test_wav(&fixture_path, 16000, 0.1).expect("failed to create test WAV");
    let config_dir = isolated_config_dir();
    write_shorten_config(config_dir.path(), true);

    // Config says on, but --shorten false should win.
    Command::cargo_bin("whspr")
        .unwrap()
        .args([
            "transcribe",
            fixture_path.to_str().unwrap(),
            "--asr",
            "mock",
            "--asr-mock-text",
            "it's kind of tired",
            "--refine",
            "noop",
            "--shorten",
            "false",
            "--no-store",
            "--config-dir",
            config_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("it's kind of tired"));
}
