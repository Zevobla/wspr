//! The actual criterion verifications, one module per theme. Each function
//! here returns zero or more `CheckResult`s; `run_all` in this module
//! collects every family into the single ordered `Vec` `main` hands to
//! `report::print`.

pub mod build;
pub mod license;
pub mod tests_isolation;

use crate::report::CheckResult;
use std::path::Path;

/// Runs every implemented check family against the repo at `root` and
/// returns all results, in catalog order (the report printer re-sorts by
/// group anyway, but keeping this in catalog order makes diffs of the raw
/// output stable).
pub fn run_all(root: &Path) -> Vec<CheckResult> {
    let mut results = Vec::new();
    results.extend(build::check_build_and_lock(root));
    results.push(build::check_clippy(root));
    results.push(build::check_fmt(root));

    let test_results = tests_isolation::check_test_suite(root);
    let tests_passed = test_results
        .iter()
        .all(|r| r.verdict == crate::report::Verdict::Pass);
    results.extend(test_results);
    results.push(tests_isolation::check_mic_model_independence(
        root,
        tests_passed,
    ));
    results.push(tests_isolation::check_ci_configured(root));

    results.push(license::check_license_file_present(root));
    results.extend(license::check_declared_license(root));

    results
}
