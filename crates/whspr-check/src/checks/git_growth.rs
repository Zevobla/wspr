//! AD: growth-monotonicity checks computed from `git log`. These all
//! target the `main` branch specifically (falling back to `HEAD` if `main`
//! doesn't resolve, e.g. a shallow clone) rather than whatever branch
//! happens to be checked out - `whspr-check` itself develops on
//! `team/acceptance`, and its own commits shouldn't skew the numbers this
//! reports about the project's history.
//!
//! Every numeric threshold below (commit count floor, max gap, etc.) is
//! this checker's own defensible-but-not-official heuristic - the real
//! acceptance matrix's exact thresholds, if any, aren't available to this
//! tool. Each check's evidence states the raw number *and* the threshold
//! applied, so a reviewer can disagree with the threshold without
//! disputing the measurement.

use crate::report::CheckResult;
use crate::repo;
use std::path::Path;

/// Resolves to `main` if it exists, else `HEAD` (e.g. a checkout that only
/// has the current branch).
fn target_ref(root: &Path) -> anyhow::Result<String> {
    let probe = repo::run(root, "git", &["rev-parse", "--verify", "main"])?;
    Ok(if probe.success {
        "main".to_string()
    } else {
        "HEAD".to_string()
    })
}

struct CommitMeta {
    hash: String,
    timestamp: i64,
    author_email: String,
}

fn log_all_commits(root: &Path, git_ref: &str) -> anyhow::Result<Vec<CommitMeta>> {
    let output = repo::run(root, "git", &["log", git_ref, "--format=%H\t%at\t%ae"])?;
    if !output.success {
        anyhow::bail!("`git log {git_ref}` failed: {}", output.stderr);
    }
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let hash = parts.next()?.to_string();
            let timestamp = parts.next()?.parse().ok()?;
            let author_email = parts.next()?.to_string();
            Some(CommitMeta {
                hash,
                timestamp,
                author_email,
            })
        })
        .collect())
}

/// AD-01: commit count reflects incremental history.
pub fn check_commit_count(root: &Path) -> CheckResult {
    let git_ref = match target_ref(root) {
        Ok(r) => r,
        Err(e) => return CheckResult::fail("AD-01", e.to_string()),
    };
    let commits = match log_all_commits(root, &git_ref) {
        Ok(c) => c,
        Err(e) => return CheckResult::fail("AD-01", e.to_string()),
    };

    const MIN_COMMITS: usize = 20;
    if commits.len() >= MIN_COMMITS {
        CheckResult::pass(
            "AD-01",
            format!(
                "{} commits on {git_ref} (threshold: >= {MIN_COMMITS}, our own heuristic floor \
                 for \"incremental\")",
                commits.len()
            ),
        )
    } else {
        CheckResult::fail(
            "AD-01",
            format!(
                "only {} commits on {git_ref} (threshold: >= {MIN_COMMITS})",
                commits.len()
            ),
        )
    }
}
