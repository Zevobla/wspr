//! CLI behavior checks: `whspr --version`/`--help`, exit codes, and
//! stdout/stderr output discipline. These build the real `whspr` binary
//! once and then invoke it directly (rather than going through `cargo run`
//! for every check), so cargo's own build noise never leaks into the
//! stdout/stderr we're inspecting.

use crate::repo::{self, CmdOutput};
use crate::report::CheckResult;
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
                    format!(
                        "`whspr --version` exited non-zero; stderr: {}",
                        out.stderr.trim()
                    ),
                )
            };
            let looks_like_version = out.stdout.to_lowercase().contains("whspr")
                && out.stdout.chars().any(|c| c.is_ascii_digit());
            let y03 = if looks_like_version {
                CheckResult::pass(
                    "Y-03",
                    format!(
                        "stdout: {:?} (contains \"whspr\" and a digit)",
                        out.stdout.trim()
                    ),
                )
            } else {
                CheckResult::fail(
                    "Y-03",
                    format!(
                        "stdout doesn't look like a version string: {:?}",
                        out.stdout.trim()
                    ),
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
/// Runs `transcribe` against a real WAV fixture (see
/// `repo::fixture_wav_path`) so the pipeline actually completes, then
/// confirms stdout is exactly the one transcript line *and* that
/// whspr-cli's progress messages ("Loading audio...", etc.) show up on
/// stderr instead. A real check now that `transcribe` actually runs end to
/// end - not merely "nothing leaked onto stdout because nothing ran",
/// which is what this looked like against `/dev/null` before whspr-cli's
/// `decode_wav` was wired up for real.
fn check_progress_output_discipline(bin: &Path, root: &Path) -> CheckResult {
    let fixture = repo::fixture_wav_path(root);
    let Some(fixture_str) = fixture.to_str() else {
        return CheckResult::fail("Y-15", "fixture WAV path isn't valid UTF-8");
    };
    match run_whspr(bin, root, &["transcribe", fixture_str]) {
        Ok(out) if out.success => {
            let stdout_lines: Vec<&str> = out.stdout.lines().collect();
            if stdout_lines.len() == 1 && !out.stderr.trim().is_empty() {
                CheckResult::pass(
                    "Y-15",
                    format!(
                        "`whspr transcribe {}` stdout is exactly one line ({:?}); progress \
                         output ({} stderr line(s)) goes to stderr instead",
                        fixture.display(),
                        stdout_lines[0],
                        out.stderr.lines().count()
                    ),
                )
            } else {
                CheckResult::fail(
                    "Y-15",
                    format!(
                        "expected exactly 1 stdout line plus non-empty stderr progress output; \
                         got {} stdout line(s) ({:?}), {} stderr line(s)",
                        stdout_lines.len(),
                        out.stdout,
                        out.stderr.lines().count()
                    ),
                )
            }
        }
        Ok(out) => CheckResult::fail(
            "Y-15",
            format!(
                "`whspr transcribe {}` exited non-zero: {}",
                fixture.display(),
                out.stderr
            ),
        ),
        Err(e) => CheckResult::fail("Y-15", format!("could not run whspr transcribe: {e}")),
    }
}

/// Y-13: CLI works headless - removes display-server env vars entirely
/// (rather than setting them empty, which some toolkits still treat as
/// "present") and confirms the mock/local transcribe path is unaffected.
fn check_headless(bin: &Path, root: &Path) -> CheckResult {
    let Some(bin_str) = bin.to_str() else {
        return CheckResult::fail("Y-13", "binary path isn't valid UTF-8");
    };
    let fixture = repo::fixture_wav_path(root);
    let Some(fixture_str) = fixture.to_str() else {
        return CheckResult::fail("Y-13", "fixture WAV path isn't valid UTF-8");
    };
    match repo::run_without_envs(
        root,
        bin_str,
        &["transcribe", fixture_str],
        &["DISPLAY", "WAYLAND_DISPLAY"],
    ) {
        Ok(out) if out.success && out.stdout.contains("the quick brown fox") => CheckResult::pass(
            "Y-13",
            "`whspr transcribe` on a real WAV fixture still succeeds with DISPLAY and \
             WAYLAND_DISPLAY removed from its environment - no GUI/display-server dependency in \
             this path",
        ),
        Ok(out) => CheckResult::fail(
            "Y-13",
            format!(
                "headless run: success={}, stdout={:?}",
                out.success, out.stdout
            ),
        ),
        Err(e) => CheckResult::fail("Y-13", format!("could not run whspr headless: {e}")),
    }
}

/// Y-14: repeat run gives identical output (determinism).
fn check_repeat_run_deterministic(bin: &Path, root: &Path) -> CheckResult {
    let fixture = repo::fixture_wav_path(root);
    let Some(fixture_str) = fixture.to_str() else {
        return CheckResult::fail("Y-14", "fixture WAV path isn't valid UTF-8");
    };
    let first = run_whspr(bin, root, &["transcribe", fixture_str]);
    let second = run_whspr(bin, root, &["transcribe", fixture_str]);
    match (first, second) {
        (Ok(a), Ok(b)) if a.success && b.success && a.stdout == b.stdout => CheckResult::pass(
            "Y-14",
            format!(
                "two consecutive `whspr transcribe` runs on the same WAV fixture produced \
                 identical stdout: {:?}",
                a.stdout.trim()
            ),
        ),
        (Ok(a), Ok(b)) => CheckResult::fail(
            "Y-14",
            format!(
                "outputs differ between two consecutive runs: run1={:?} run2={:?}",
                a.stdout, b.stdout
            ),
        ),
        _ => CheckResult::fail("Y-14", "could not run whspr twice to compare"),
    }
}

const CLI_CRITERIA: &[&str] = &["Y-11", "Y-03", "Y-12", "Y-04", "Y-15", "Y-13", "Y-14"];

/// Runs every CLI-behavior check against an already-built `whspr` binary
/// (built once by the caller via `repo::ensure_binary_built`, and shared
/// with `checks::privacy`'s P-01, which also needs to invoke it).
pub fn run_cli_checks(bin: &Path, root: &Path) -> Vec<CheckResult> {
    let mut results = check_version(bin, root);
    results.push(check_help(bin, root));
    results.push(check_nonzero_exit_on_error(bin, root));
    results.push(check_progress_output_discipline(bin, root));
    results.push(check_headless(bin, root));
    results.push(check_repeat_run_deterministic(bin, root));
    results
}

/// Failure result for every CLI criterion, used when the `whspr` binary
/// itself couldn't be built.
pub fn build_failure_results(reason: &str) -> Vec<CheckResult> {
    CLI_CRITERIA
        .iter()
        .copied()
        .map(|id| CheckResult::fail(id, format!("could not build whspr-cli: {reason}")))
        .collect()
}
