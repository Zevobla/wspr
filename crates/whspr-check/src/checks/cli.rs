//! CLI behavior checks: `whspr --version`/`--help`, exit codes, and
//! stdout/stderr output discipline. These build the real `whspr` binary
//! once and then invoke it directly (rather than going through `cargo run`
//! for every check), so cargo's own build noise never leaks into the
//! stdout/stderr we're inspecting.

use crate::report::CheckResult;
use crate::repo::{self, CmdOutput};
use std::path::Path;

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

/// Y-15: progress/log output goes to stderr, not stdout.
///
/// `whspr transcribe` doesn't emit any progress/state-transition output
/// today (`whspr-cli`'s `main.rs` never wires a `StateCallback` into the
/// `Pipeline`) - so this can't yet verify "progress goes to stderr" in the
/// strong sense of *checking a progress line's destination*. What it does
/// check, honestly: stdout carries exactly the one final transcript line
/// and nothing else, i.e. today's total absence of progress output at
/// least isn't leaking onto stdout by accident. This needs re-checking
/// once a state callback is actually wired in - see the evidence text.
fn check_progress_output_discipline(bin: &Path, root: &Path) -> CheckResult {
    match run_whspr(bin, root, &["transcribe", "/dev/null"]) {
        Ok(out) if out.success => {
            let stdout_lines: Vec<&str> = out.stdout.lines().collect();
            if stdout_lines.len() == 1 {
                CheckResult::pass(
                    "Y-15",
                    format!(
                        "`whspr transcribe` stdout is exactly one line ({:?}); no progress/log \
                         noise on stdout. Caveat: no progress reporting is wired up at all yet \
                         (no StateCallback in whspr-cli's main.rs), so this only confirms \
                         stdout stays clean *today* - re-check once progress output exists",
                        stdout_lines[0]
                    ),
                )
            } else {
                CheckResult::fail(
                    "Y-15",
                    format!(
                        "expected exactly 1 stdout line (just the transcript), got {}: {:?}",
                        stdout_lines.len(),
                        out.stdout
                    ),
                )
            }
        }
        Ok(out) => CheckResult::fail(
            "Y-15",
            format!("`whspr transcribe /dev/null` exited non-zero: {}", out.stderr),
        ),
        Err(e) => CheckResult::fail("Y-15", format!("could not run whspr transcribe: {e}")),
    }
}

const CLI_CRITERIA: &[&str] = &["Y-11", "Y-03", "Y-12", "Y-04", "Y-15"];

/// Runs every CLI-behavior check against an already-built `whspr` binary
/// (built once by the caller via `repo::ensure_binary_built`, and shared
/// with `checks::privacy`'s P-01, which also needs to invoke it).
pub fn run_cli_checks(bin: &Path, root: &Path) -> Vec<CheckResult> {
    let mut results = check_version(bin, root);
    results.push(check_help(bin, root));
    results.push(check_nonzero_exit_on_error(bin, root));
    results.push(check_progress_output_discipline(bin, root));
    results
}

/// Failure result for every CLI criterion, used when the `whspr` binary
/// itself couldn't be built.
pub fn build_failure_results(reason: &str) -> Vec<CheckResult> {
    CLI_CRITERIA
        .iter()
        .map(|id| CheckResult::fail(*id, format!("could not build whspr-cli: {reason}")))
        .collect()
}
