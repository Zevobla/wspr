//! CLI behavior checks: `whspr --version`/`--help`, exit codes, and
//! stdout/stderr output discipline. These build the real `whspr` binary
//! once and then invoke it directly (rather than going through `cargo run`
//! for every check), so cargo's own build noise never leaks into the
//! stdout/stderr we're inspecting.

use crate::report::CheckResult;
use crate::repo::{self, CmdOutput};
use std::path::{Path, PathBuf};

/// Builds `whspr-cli` and returns the path to the resulting `whspr`
/// binary. Assumes the default `target/` layout (no custom
/// `CARGO_TARGET_DIR`), which matches every documented dev workflow in
/// this repo (README, CLAUDE.md, flake devShell).
fn ensure_whspr_binary(root: &Path) -> anyhow::Result<PathBuf> {
    let build = repo::run(root, "cargo", &["build", "-p", "whspr-cli", "--quiet"])?;
    if !build.success {
        anyhow::bail!("`cargo build -p whspr-cli` failed: {}", build.stderr);
    }
    let path = root.join("target/debug/whspr");
    if !path.is_file() {
        anyhow::bail!(
            "expected a binary at {} after building whspr-cli, but it's not there (custom \
             CARGO_TARGET_DIR?)",
            path.display()
        );
    }
    Ok(path)
}

fn run_whspr(bin: &Path, root: &Path, args: &[&str]) -> anyhow::Result<CmdOutput> {
    let bin_str = bin
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("binary path {} isn't valid UTF-8", bin.display()))?;
    repo::run(root, bin_str, args)
}

/// Y-11 (whspr --version exits zero) and Y-03 (whspr --version prints a
/// version string), from one invocation.
fn check_version(bin: &Path, root: &Path) -> Vec<CheckResult> {
    match run_whspr(bin, root, &["--version"]) {
        Ok(out) => {
            let y11 = if out.success {
                CheckResult::pass("Y-11", "`whspr --version` exited 0")
            } else {
                CheckResult::fail(
                    "Y-11",
                    format!("`whspr --version` exited non-zero; stderr: {}", out.stderr.trim()),
                )
            };
            let looks_like_version =
                out.stdout.to_lowercase().contains("whspr") && out.stdout.chars().any(|c| c.is_ascii_digit());
            let y03 = if looks_like_version {
                CheckResult::pass(
                    "Y-03",
                    format!("stdout: {:?} (contains \"whspr\" and a digit)", out.stdout.trim()),
                )
            } else {
                CheckResult::fail(
                    "Y-03",
                    format!("stdout doesn't look like a version string: {:?}", out.stdout.trim()),
                )
            };
            vec![y11, y03]
        }
        Err(e) => vec![
            CheckResult::fail("Y-11", format!("could not run `whspr --version`: {e}")),
            CheckResult::fail("Y-03", format!("could not run `whspr --version`: {e}")),
        ],
    }
}

/// Y-12: `whspr --help` works (exits zero, mentions usage).
fn check_help(bin: &Path, root: &Path) -> CheckResult {
    match run_whspr(bin, root, &["--help"]) {
        Ok(out) if out.success && out.stdout.to_lowercase().contains("usage") => {
            CheckResult::pass("Y-12", "`whspr --help` exited 0 and printed a Usage: line")
        }
        Ok(out) => CheckResult::fail(
            "Y-12",
            format!(
                "`whspr --help` exit success={}, stdout mentions \"usage\": {}",
                out.success,
                out.stdout.to_lowercase().contains("usage")
            ),
        ),
        Err(e) => CheckResult::fail("Y-12", format!("could not run `whspr --help`: {e}")),
    }
}

/// Y-04: CLI exits non-zero on error (an unrecognized subcommand).
fn check_nonzero_exit_on_error(bin: &Path, root: &Path) -> CheckResult {
    match run_whspr(bin, root, &["this-subcommand-does-not-exist"]) {
        Ok(out) if !out.success => CheckResult::pass(
            "Y-04",
            format!(
                "`whspr this-subcommand-does-not-exist` exited non-zero (code {:?})",
                out.code
            ),
        ),
        Ok(_) => CheckResult::fail("Y-04", "an unrecognized subcommand exited zero"),
        Err(e) => CheckResult::fail("Y-04", format!("could not invoke whspr: {e}")),
    }
}

const CLI_CRITERIA: &[&str] = &["Y-11", "Y-03", "Y-12", "Y-04"];

/// Runs every CLI-behavior check against one build of the `whspr` binary.
pub fn run_cli_checks(root: &Path) -> Vec<CheckResult> {
    let bin = match ensure_whspr_binary(root) {
        Ok(p) => p,
        Err(e) => {
            return CLI_CRITERIA
                .iter()
                .map(|id| CheckResult::fail(*id, format!("could not build whspr-cli: {e}")))
                .collect()
        }
    };

    let mut results = check_version(&bin, root);
    results.push(check_help(&bin, root));
    results.push(check_nonzero_exit_on_error(&bin, root));
    results
}
