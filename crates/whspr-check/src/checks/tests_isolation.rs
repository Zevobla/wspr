//! Checks about the test suite itself: does it pass, is it isolated from
//! the network, does it need a mic/model files, does it run in CI.

use crate::repo::{self, CmdOutput};
use crate::report::CheckResult;
use crate::util::{tail, MODEL_WEIGHT_EXTENSIONS};
use std::collections::HashMap;
use std::path::Path;

/// Bogus proxy env vars that make any HTTP client routing through a system
/// proxy (reqwest does, by default) fail fast on any *real* outbound
/// request, while `NO_PROXY` exempts localhost so a wiremock `MockServer`
/// (which binds to 127.0.0.1) keeps working. If the suite still passes
/// under this, no test actually depends on reaching a real host.
const POISON_ENV: &[(&str, &str)] = &[
    ("HTTP_PROXY", "http://127.0.0.1:9"),
    ("HTTPS_PROXY", "http://127.0.0.1:9"),
    ("ALL_PROXY", "http://127.0.0.1:9"),
    ("NO_PROXY", "127.0.0.1,localhost"),
];

fn run_tests_with_poisoned_network(root: &Path) -> anyhow::Result<CmdOutput> {
    repo::run_env(root, "cargo", &["test", "--workspace"], POISON_ENV)
}

/// Known CI config locations across common providers. Presence of any one
/// of these is treated as "tests run in CI" - we don't attempt to verify
/// the workflow actually runs the test suite (that would mean parsing
/// arbitrary YAML/pipeline DSLs), only that some CI is wired up at all.
const CI_CONFIG_PATHS: &[&str] = &[
    ".github/workflows",
    ".gitlab-ci.yml",
    ".circleci/config.yml",
    "azure-pipelines.yml",
    ".travis.yml",
    "Jenkinsfile",
];

/// AB-15: tests run in CI.
pub fn check_ci_configured(root: &Path) -> CheckResult {
    let found: Vec<&str> = CI_CONFIG_PATHS
        .iter()
        .filter(|p| {
            let path = root.join(p);
            if path.is_dir() {
                // A workflows/ dir only counts if it actually has a
                // workflow file in it, not just an empty directory.
                path.read_dir()
                    .map(|mut entries| entries.next().is_some())
                    .unwrap_or(false)
            } else {
                path.is_file()
            }
        })
        .copied()
        .collect();

    if found.is_empty() {
        CheckResult::fail(
            "AB-15",
            format!(
                "no CI config found at any of: {}",
                CI_CONFIG_PATHS.join(", ")
            ),
        )
    } else {
        CheckResult::pass("AB-15", format!("found CI config at: {}", found.join(", ")))
    }
}

/// AB-06: unit tests run without a mic or model files present.
///
/// `tests_passed` is threaded in from `check_test_suite` rather than
/// re-running the suite a third time; this function's own job is the two
/// static facts that make a passing suite *mean* "doesn't need a mic or
/// model": no model-weight file is tracked in the repo, and the one real
/// microphone entry point (`whspr_audio::start_capture`) is never called
/// from test code (its own definition, or real application code like
/// whspr-app's worker calling it when a user is actually recording, don't
/// count - only a call reachable from `cargo test` would mean the suite
/// needs a live mic).
pub fn check_mic_model_independence(root: &Path, tests_passed: bool) -> CheckResult {
    if !tests_passed {
        return CheckResult::fail(
            "AB-06",
            "test suite did not pass (see A-13); can't credit mic/model independence",
        );
    }

    let tracked = match repo::git_ls_files(root) {
        Ok(files) => files,
        Err(e) => return CheckResult::fail("AB-06", format!("could not list tracked files: {e}")),
    };
    let model_files: Vec<&String> = tracked
        .iter()
        .filter(|p| {
            let lower = p.to_lowercase();
            MODEL_WEIGHT_EXTENSIONS
                .iter()
                .any(|ext| lower.ends_with(ext))
        })
        .collect();
    if !model_files.is_empty() {
        return CheckResult::fail(
            "AB-06",
            format!(
                "model-weight-shaped files are tracked in git: {}",
                model_files
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }

    let capture_refs = match repo::git_grep(root, &["-F"], "start_capture(", &[]) {
        Ok(lines) => lines,
        Err(e) => {
            return CheckResult::fail("AB-06", format!("could not grep for start_capture(: {e}"))
        }
    };
    // What this check actually cares about is whether the *unit test suite*
    // needs a live mic - not whether any application code ever calls
    // start_capture() at all (whspr-app's worker legitimately does, when a
    // user is actually recording; that's the whole point of the function).
    // So beyond excluding the definition itself, only keep hits that are
    // genuine test code: a file under any `tests/` directory (always
    // test-only), or - for an inline unit-test module - a line at or past
    // that same file's own `#[cfg(test)]` marker (every test module in this
    // repo is a trailing block, matching AA-16's identical convention).
    let mut test_marker_line_by_path: HashMap<String, usize> = HashMap::new();
    let call_sites: Vec<&String> = capture_refs
        .iter()
        .filter(|line| !line.contains("fn start_capture"))
        .filter(|line| {
            let mut parts = line.splitn(3, ':');
            let (Some(path), Some(lineno_str)) = (parts.next(), parts.next()) else {
                return true; // malformed line - don't silently drop a possible finding
            };
            if path.split('/').any(|component| component == "tests") {
                return true; // e.g. crates/*/tests/*.rs - always test code
            }
            let Ok(lineno) = lineno_str.parse::<usize>() else {
                return true;
            };
            let test_line = *test_marker_line_by_path
                .entry(path.to_string())
                .or_insert_with(|| {
                    repo::git_grep(root, &["-F", "-m", "1"], "#[cfg(test)]", &[path])
                        .ok()
                        .and_then(|m| m.first()?.split(':').nth(1)?.parse::<usize>().ok())
                        .unwrap_or(usize::MAX)
                });
            lineno >= test_line
        })
        .collect();
    if !call_sites.is_empty() {
        return CheckResult::fail(
            "AB-06",
            format!(
                "start_capture() is invoked from test code (possible live-mic dependency): {}",
                call_sites
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }

    CheckResult::pass(
        "AB-06",
        format!(
            "suite passes; 0 tracked model-weight files ({} extensions checked); \
             start_capture() has {} tracked reference(s), none of them from test code (any \
             non-definition call sites are real application code, e.g. whspr-app's worker \
             calling it while actually recording)",
            MODEL_WEIGHT_EXTENSIONS.len(),
            capture_refs.len()
        ),
    )
}

/// A-13 (tests pass with a single command) and AB-13 (tests are isolated
/// from the network), verified by the same subprocess run: we run the
/// *whole* suite once, under the poisoned-network guard above. A plain
/// `cargo test --workspace` pass wouldn't tell us the suite is
/// network-independent; this does, and it's strictly a superset of what
/// A-13 asks for, so we don't run the suite twice.
pub fn check_test_suite(root: &Path) -> Vec<CheckResult> {
    match run_tests_with_poisoned_network(root) {
        Ok(output) if output.success => vec![
            CheckResult::pass(
                "A-13",
                "`cargo test --workspace` exited 0 (run under a poisoned-network guard, see AB-13)",
            ),
            CheckResult::pass(
                "AB-13",
                "`cargo test --workspace` still passes with HTTP_PROXY/HTTPS_PROXY/ALL_PROXY \
                 pointed at an unreachable address (NO_PROXY exempts 127.0.0.1 so wiremock's \
                 local mock servers still work) - no test depends on reaching a real host",
            ),
        ],
        Ok(output) => {
            let evidence = format!(
                "`cargo test --workspace` failed under the poisoned-network guard; last 800 \
                 chars of stdout: {}",
                tail(&output.stdout, 800)
            );
            vec![
                CheckResult::fail("A-13", evidence.clone()),
                CheckResult::fail(
                    "AB-13",
                    format!(
                        "{evidence} (failure under a poisoned proxy may indicate a real \
                             network dependency, or may just be the A-13 failure itself - see \
                             the A-13 evidence)"
                    ),
                ),
            ]
        }
        Err(e) => vec![
            CheckResult::fail("A-13", format!("could not run cargo test: {e}")),
            CheckResult::fail("AB-13", format!("could not run cargo test: {e}")),
        ],
    }
}

/// Per-test outcome lines (`test <name> ... ok`/`FAILED`/`ignored`) from a
/// `cargo test` run's stdout, sorted - used by AB-12 to compare two runs by
/// *what happened*, not by raw byte-for-byte stdout (which would also
/// vary on cosmetic things like parallel-test print ordering that have
/// nothing to do with whether the tests themselves are flaky).
fn extract_test_outcomes(stdout: &str) -> Vec<String> {
    let mut outcomes: Vec<String> = stdout
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            // Per-test lines look like "test some::path ... ok" - distinct
            // from the "test result: ok. N passed; ...; finished in X.XXs"
            // summary line, which legitimately varies in timing between
            // runs and must not be compared here.
            l.starts_with("test ")
                && !l.starts_with("test result:")
                && (l.contains(" ... ok") || l.contains(" ... FAILED"))
        })
        .map(|l| l.trim().to_string())
        .collect();
    outcomes.sort();
    outcomes
}

/// AB-05 (test-suite runtime is bounded) and AB-12 (tests give identical
/// outcomes across repeat runs), from two plain, unpoisoned `cargo test
/// --workspace` runs.
///
/// Deliberately separate runs from `check_test_suite`'s poisoned-network
/// one above: AB-05/AB-12 are about wall-clock time and outcome stability,
/// not network isolation, and timing/comparing a poisoned run wouldn't
/// represent normal behavior.
///
/// AB-05's 120s threshold is this checker's own defensible heuristic
/// (matching the AD-group checks' style): today's suite runs in ~1s,
/// leaving generous room to grow before flagging anything.
pub fn check_test_suite_runtime_and_determinism(root: &Path) -> Vec<CheckResult> {
    let start = std::time::Instant::now();
    let first = repo::run(root, "cargo", &["test", "--workspace"]);
    let elapsed = start.elapsed();

    const MAX_SECS: f64 = 120.0;
    let ab05 = match &first {
        Ok(out) if out.success => {
            if elapsed.as_secs_f64() <= MAX_SECS {
                CheckResult::pass(
                    "AB-05",
                    format!(
                        "`cargo test --workspace` completed in {:.2}s (threshold: <= \
                         {MAX_SECS:.0}s)",
                        elapsed.as_secs_f64()
                    ),
                )
            } else {
                CheckResult::fail(
                    "AB-05",
                    format!(
                        "`cargo test --workspace` took {:.2}s (threshold: <= {MAX_SECS:.0}s)",
                        elapsed.as_secs_f64()
                    ),
                )
            }
        }
        Ok(out) => CheckResult::fail(
            "AB-05",
            format!(
                "test suite failed, so its runtime isn't a meaningful measurement; stderr tail: \
                 {}",
                tail(&out.stderr, 400)
            ),
        ),
        Err(e) => CheckResult::fail("AB-05", format!("could not run cargo test: {e}")),
    };

    let second = repo::run(root, "cargo", &["test", "--workspace"]);
    let ab12 = match (&first, &second) {
        (Ok(a), Ok(b)) => {
            let outcomes_a = extract_test_outcomes(&a.stdout);
            let outcomes_b = extract_test_outcomes(&b.stdout);
            if !outcomes_a.is_empty() && outcomes_a == outcomes_b {
                CheckResult::pass(
                    "AB-12",
                    format!(
                        "{} individual test outcomes (name + pass/fail) are identical across \
                         two consecutive `cargo test --workspace` runs",
                        outcomes_a.len()
                    ),
                )
            } else if outcomes_a.is_empty() {
                CheckResult::fail(
                    "AB-12",
                    "could not extract any per-test outcome lines from cargo test output to \
                     compare",
                )
            } else {
                use std::collections::HashSet;
                let set_a: HashSet<&String> = outcomes_a.iter().collect();
                let set_b: HashSet<&String> = outcomes_b.iter().collect();
                let only_first: Vec<&&String> = set_a.difference(&set_b).collect();
                let only_second: Vec<&&String> = set_b.difference(&set_a).collect();
                CheckResult::fail(
                    "AB-12",
                    format!(
                        "test outcomes differ between two consecutive runs: {} outcome(s) only \
                         in run 1, {} only in run 2 - e.g. {:?} / {:?}",
                        only_first.len(),
                        only_second.len(),
                        only_first.first(),
                        only_second.first()
                    ),
                )
            }
        }
        _ => CheckResult::fail("AB-12", "could not run cargo test twice to compare"),
    };

    vec![ab05, ab12]
}
