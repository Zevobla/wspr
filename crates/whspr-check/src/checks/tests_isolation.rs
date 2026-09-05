//! Checks about the test suite itself: does it pass, is it isolated from
//! the network, does it need a mic/model files, does it run in CI.

use crate::report::CheckResult;
use crate::repo::{self, CmdOutput};
use crate::util::{tail, MODEL_WEIGHT_EXTENSIONS};
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
/// anywhere except its own definition (i.e. not from a test).
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
            MODEL_WEIGHT_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
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

    let capture_refs = match repo::git_grep(root, &["-F"], "start_capture(") {
        Ok(lines) => lines,
        Err(e) => {
            return CheckResult::fail("AB-06", format!("could not grep for start_capture(: {e}"))
        }
    };
    let call_sites: Vec<&String> = capture_refs
        .iter()
        .filter(|line| !line.contains("fn start_capture"))
        .collect();
    if !call_sites.is_empty() {
        return CheckResult::fail(
            "AB-06",
            format!(
                "start_capture() is invoked outside its own definition (possible live-mic \
                 dependency): {}",
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
             start_capture() has {} tracked reference(s), all at its own definition (no test \
             invokes it)",
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
                    format!("{evidence} (failure under a poisoned proxy may indicate a real \
                             network dependency, or may just be the A-13 failure itself - see \
                             the A-13 evidence)"),
                ),
            ]
        }
        Err(e) => vec![
            CheckResult::fail("A-13", format!("could not run cargo test: {e}")),
            CheckResult::fail("AB-13", format!("could not run cargo test: {e}")),
        ],
    }
}
