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

/// Median of a sorted-ascending slice of i64s (already-sorted precondition
/// kept private to this module - the one caller sorts right before calling).
fn median_of_sorted(sorted: &[i64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2] as f64
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0
    }
}

/// AD-02 (median inter-commit interval indicates organic pacing) and AD-03
/// (max gap between commits is bounded), computed from the same sorted
/// list of commit timestamps on one `git log` call.
pub fn check_commit_timing(root: &Path) -> Vec<CheckResult> {
    let git_ref = match target_ref(root) {
        Ok(r) => r,
        Err(e) => {
            return vec![
                CheckResult::fail("AD-02", e.to_string()),
                CheckResult::fail("AD-03", e.to_string()),
            ]
        }
    };
    let commits = match log_all_commits(root, &git_ref) {
        Ok(c) => c,
        Err(e) => {
            return vec![
                CheckResult::fail("AD-02", e.to_string()),
                CheckResult::fail("AD-03", e.to_string()),
            ]
        }
    };

    let mut timestamps: Vec<i64> = commits.iter().map(|c| c.timestamp).collect();
    timestamps.sort_unstable();

    if timestamps.len() < 2 {
        let evidence = format!("only {} commit(s) on {git_ref}; need >= 2 to measure gaps", timestamps.len());
        return vec![
            CheckResult::needs_bench("AD-02", evidence.clone()),
            CheckResult::needs_bench("AD-03", evidence),
        ];
    }

    let mut deltas: Vec<i64> = timestamps.windows(2).map(|w| w[1] - w[0]).collect();
    deltas.sort_unstable();
    let median = median_of_sorted(&deltas);
    let max_gap = *deltas.last().expect("checked len >= 2 above, so >= 1 delta");

    const MIN_MEDIAN_SECS: f64 = 1.0;
    let ad02 = if median >= MIN_MEDIAN_SECS {
        CheckResult::pass(
            "AD-02",
            format!(
                "median inter-commit interval is {median:.0}s across {} gaps (threshold: >= \
                 {MIN_MEDIAN_SECS:.0}s, ruling out sub-second/scripted timestamps)",
                deltas.len()
            ),
        )
    } else {
        CheckResult::fail(
            "AD-02",
            format!(
                "median inter-commit interval is {median:.0}s, below the {MIN_MEDIAN_SECS:.0}s \
                 threshold - looks scripted rather than organic"
            ),
        )
    };

    const MAX_GAP_SECS: i64 = 24 * 3600;
    let ad03 = if max_gap <= MAX_GAP_SECS {
        CheckResult::pass(
            "AD-03",
            format!(
                "max gap between consecutive commits is {:.1}h (threshold: <= {}h)",
                max_gap as f64 / 3600.0,
                MAX_GAP_SECS / 3600
            ),
        )
    } else {
        CheckResult::fail(
            "AD-03",
            format!(
                "max gap between consecutive commits is {:.1}h, over the {}h threshold",
                max_gap as f64 / 3600.0,
                MAX_GAP_SECS / 3600
            ),
        )
    };

    vec![ad02, ad03]
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
