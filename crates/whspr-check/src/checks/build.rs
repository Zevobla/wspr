//! Build-health checks: does the workspace build, is the lockfile
//! committed, is it lint/format clean. These shell out to `cargo` against
//! the repo root found by `crate::repo::find_repo_root`.

use crate::report::CheckResult;
use crate::util::{head, tail};
use std::path::Path;

/// A-01 (one-command build works) and A-03 (Cargo.lock present).
///
/// A-01 specifically validates the `cargo build --workspace` path (fast,
/// reliable, no network/cache needed) rather than `nix build`: `nix build`
/// also works per the README, but depends on flake-input availability we
/// can't assume in every environment this checker runs in, so we don't
/// silently substitute a slower/flakier check for what the README actually
/// promises as the primary path.
pub fn check_build_and_lock(root: &Path) -> Vec<CheckResult> {
    let mut out = Vec::new();

    let lockfile = root.join("Cargo.lock");
    if lockfile.is_file() {
        out.push(CheckResult::pass(
            "A-03",
            format!(
                "{} exists and is presumably tracked by git",
                lockfile.display()
            ),
        ));
    } else {
        out.push(CheckResult::fail(
            "A-03",
            format!("{} does not exist", lockfile.display()),
        ));
    }

    match crate::repo::run(root, "cargo", &["build", "--workspace"]) {
        Ok(output) if output.success => {
            out.push(CheckResult::pass(
                "A-01",
                "`cargo build --workspace` exited 0",
            ));
        }
        Ok(output) => {
            out.push(CheckResult::fail(
                "A-01",
                format!(
                    "`cargo build --workspace` failed; last 500 chars of stderr: {}",
                    tail(&output.stderr, 500)
                ),
            ));
        }
        Err(e) => {
            out.push(CheckResult::fail(
                "A-01",
                format!("could not run `cargo build --workspace`: {e}"),
            ));
        }
    }

    out
}

/// Dead-code-shaped lint names rustc/clippy actually uses, checked against
/// a failing clippy run's stderr to tell "AC-06 specifically failed" apart
/// from "clippy failed for some unrelated reason."
const DEAD_CODE_MARKERS: &[&str] = &["dead_code", "never used", "never read", "never constructed"];

/// AA-13 (`cargo clippy --workspace --all-targets -- -D warnings` is
/// clean) and AC-06 (dead code absent), from the same clippy run: `-D
/// warnings` promotes every warning - including the built-in `dead_code`
/// lint - to a hard error, so a clean exit is already proof no dead code
/// was flagged. No need to run clippy a second time with a narrower lint
/// selection.
pub fn check_clippy(root: &Path) -> Vec<CheckResult> {
    match crate::repo::run(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ) {
        Ok(output) if output.success => vec![
            CheckResult::pass(
                "AA-13",
                "`cargo clippy --workspace --all-targets -- -D warnings` exited 0",
            ),
            CheckResult::pass(
                "AC-06",
                "the same clean clippy -D warnings run (see AA-13) promotes the built-in \
                 dead_code lint to an error too, so a clean exit already proves no dead code \
                 was flagged",
            ),
        ],
        Ok(output) => {
            let evidence = tail(&output.stderr, 800);
            let is_dead_code = DEAD_CODE_MARKERS.iter().any(|m| output.stderr.contains(m));
            vec![
                CheckResult::fail(
                    "AA-13",
                    format!(
                        "clippy reported warnings/errors; last 800 chars of stderr: {evidence}"
                    ),
                ),
                if is_dead_code {
                    CheckResult::fail(
                        "AC-06",
                        format!("clippy's dead-code-shaped lints fired: {evidence}"),
                    )
                } else {
                    CheckResult::fail(
                        "AC-06",
                        "AA-13's clippy run failed for a non-dead-code reason (see AA-13's \
                         evidence), so AC-06 can't be independently confirmed clean from this run",
                    )
                },
            ]
        }
        Err(e) => vec![
            CheckResult::fail("AA-13", format!("could not run cargo clippy: {e}")),
            CheckResult::fail("AC-06", format!("could not run cargo clippy: {e}")),
        ],
    }
}

/// AA-14: `cargo fmt --all -- --check` is clean.
pub fn check_fmt(root: &Path) -> CheckResult {
    match crate::repo::run(root, "cargo", &["fmt", "--all", "--", "--check"]) {
        Ok(output) if output.success => {
            CheckResult::pass("AA-14", "`cargo fmt --all -- --check` exited 0 (no diff)")
        }
        Ok(output) => CheckResult::fail(
            "AA-14",
            format!(
                "formatting differs from rustfmt's; first 800 chars of stdout diff: {}",
                head(&output.stdout, 800)
            ),
        ),
        Err(e) => CheckResult::fail("AA-14", format!("could not run cargo fmt: {e}")),
    }
}
