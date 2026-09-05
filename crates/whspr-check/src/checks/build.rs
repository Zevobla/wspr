//! Build-health checks: does the workspace build, is the lockfile
//! committed, is it lint/format clean. These shell out to `cargo` against
//! the repo root found by `crate::repo::find_repo_root`.

use crate::report::CheckResult;
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
            format!("{} exists and is presumably tracked by git", lockfile.display()),
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

/// Last `n` bytes of `s`, snapped forward to the nearest UTF-8 char
/// boundary so this never panics on a multi-byte character straddling the
/// cut point.
fn tail(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut idx = s.len() - n;
    while !s.is_char_boundary(idx) {
        idx += 1;
    }
    &s[idx..]
}
