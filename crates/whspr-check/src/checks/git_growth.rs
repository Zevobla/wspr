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

struct RsCommitShape {
    hash: String,
    subject: String,
    lines_changed: u64,
    adds_test_attr: bool,
}

const TEST_ATTR_MARKERS: &[&str] = &["#[test]", "#[tokio::test]", "#[async_std::test]"];

/// Walks one `git log -p` pass restricted to `*.rs` paths and derives, per
/// commit, how many lines changed and whether it added a test-attributed
/// function. One subprocess call serves both AD-04 and AD-07 below, rather
/// than running `git log` twice with different flags for the same commits.
fn log_rs_commit_shapes(root: &Path, git_ref: &str) -> anyhow::Result<Vec<RsCommitShape>> {
    let output = repo::run(
        root,
        "git",
        &[
            "log",
            git_ref,
            "-p",
            "--format=COMMIT\t%H\t%s",
            "--",
            "*.rs",
        ],
    )?;
    if !output.success {
        anyhow::bail!("`git log {git_ref} -p -- '*.rs'` failed: {}", output.stderr);
    }

    let mut shapes: Vec<RsCommitShape> = Vec::new();
    for line in output.stdout.lines() {
        if let Some(rest) = line.strip_prefix("COMMIT\t") {
            let mut parts = rest.splitn(2, '\t');
            let hash = parts.next().unwrap_or_default().to_string();
            let subject = parts.next().unwrap_or_default().to_string();
            shapes.push(RsCommitShape {
                hash,
                subject,
                lines_changed: 0,
                adds_test_attr: false,
            });
            continue;
        }
        let Some(current) = shapes.last_mut() else {
            continue;
        };
        if line.starts_with("+++") || line.starts_with("---") {
            continue; // diff file-header lines, not content
        }
        if let Some(added) = line.strip_prefix('+') {
            current.lines_changed += 1;
            if TEST_ATTR_MARKERS.iter().any(|m| added.contains(m)) {
                current.adds_test_attr = true;
            }
        } else if line.starts_with('-') {
            current.lines_changed += 1;
        }
    }
    Ok(shapes)
}

/// AD-04 (no single commit dominates the .rs change volume) and AD-07
/// (tests land alongside the code they cover), computed from the same
/// walk of `*.rs` history.
///
/// AD-04 deliberately counts only `.rs` files: `Cargo.lock`/`flake.lock`
/// regenerate in bulk on every dependency bump and `LICENSE` is one-time
/// boilerplate, neither of which is "code" in the sense this criterion
/// means - counting them made the repo's actual initial-scaffolding
/// commit and a couple of `cargo add` commits look like they dumped 15-20%
/// of the *entire tracked tree*, when the real source-code picture is much
/// more evenly spread.
pub fn check_rs_commit_shape(root: &Path) -> Vec<CheckResult> {
    let git_ref = match target_ref(root) {
        Ok(r) => r,
        Err(e) => {
            return vec![
                CheckResult::fail("AD-04", e.to_string()),
                CheckResult::fail("AD-07", e.to_string()),
            ]
        }
    };
    let shapes = match log_rs_commit_shapes(root, &git_ref) {
        Ok(s) => s,
        Err(e) => {
            return vec![
                CheckResult::fail("AD-04", e.to_string()),
                CheckResult::fail("AD-07", e.to_string()),
            ]
        }
    };

    if shapes.is_empty() {
        let evidence = format!("no commit on {git_ref} touches a *.rs file");
        return vec![
            CheckResult::needs_bench("AD-04", evidence.clone()),
            CheckResult::needs_bench("AD-07", evidence),
        ];
    }

    let total: u64 = shapes.iter().map(|s| s.lines_changed).sum();
    let biggest = shapes
        .iter()
        .max_by_key(|s| s.lines_changed)
        .expect("checked non-empty above");
    let fraction = if total == 0 {
        0.0
    } else {
        biggest.lines_changed as f64 / total as f64
    };

    const MAX_FRACTION: f64 = 0.30;
    let ad04 = if fraction < MAX_FRACTION {
        CheckResult::pass(
            "AD-04",
            format!(
                "largest single commit ({:.7} \"{}\") is {:.1}% of all .rs line changes across \
                 {} commits (threshold: < {:.0}%)",
                biggest.hash,
                biggest.subject,
                fraction * 100.0,
                shapes.len(),
                MAX_FRACTION * 100.0
            ),
        )
    } else {
        CheckResult::fail(
            "AD-04",
            format!(
                "largest single commit ({:.7} \"{}\") is {:.1}% of all .rs line changes \
                 (threshold: < {:.0}%)",
                biggest.hash,
                biggest.subject,
                fraction * 100.0,
                MAX_FRACTION * 100.0
            ),
        )
    };

    let with_tests = shapes.iter().filter(|s| s.adds_test_attr).count();
    let test_fraction = with_tests as f64 / shapes.len() as f64;
    const MIN_TEST_FRACTION: f64 = 0.15;
    let ad07 = if test_fraction >= MIN_TEST_FRACTION {
        CheckResult::pass(
            "AD-07",
            format!(
                "{with_tests}/{} (.rs-touching) commits add a #[test]-style function \
                 ({:.0}%, threshold: >= {:.0}%) - tests are landing spread across the history, \
                 not dumped in one bulk commit",
                shapes.len(),
                test_fraction * 100.0,
                MIN_TEST_FRACTION * 100.0
            ),
        )
    } else {
        CheckResult::fail(
            "AD-07",
            format!(
                "only {with_tests}/{} (.rs-touching) commits add a #[test]-style function \
                 ({:.0}%, threshold: >= {:.0}%)",
                shapes.len(),
                test_fraction * 100.0,
                MIN_TEST_FRACTION * 100.0
            ),
        )
    };

    vec![ad04, ad07]
}

/// AD-11: commit authorship is a single consistent identity.
pub fn check_single_authorship(root: &Path) -> CheckResult {
    let git_ref = match target_ref(root) {
        Ok(r) => r,
        Err(e) => return CheckResult::fail("AD-11", e.to_string()),
    };
    let commits = match log_all_commits(root, &git_ref) {
        Ok(c) => c,
        Err(e) => return CheckResult::fail("AD-11", e.to_string()),
    };

    let mut authors: Vec<&str> = commits.iter().map(|c| c.author_email.as_str()).collect();
    authors.sort_unstable();
    authors.dedup();

    if authors.len() == 1 {
        CheckResult::pass(
            "AD-11",
            format!("all {} commits on {git_ref} are authored by {}", commits.len(), authors[0]),
        )
    } else {
        CheckResult::fail(
            "AD-11",
            format!(
                "{} distinct author identities on {git_ref}: {}",
                authors.len(),
                authors.join(", ")
            ),
        )
    }
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
