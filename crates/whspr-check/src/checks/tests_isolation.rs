//! Checks about the test suite itself: does it pass, is it isolated from
//! the network, does it need a mic/model files, does it run in CI.

use crate::report::CheckResult;
use crate::repo::{self, CmdOutput};
use crate::util::tail;
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
