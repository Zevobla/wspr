//! The actual criterion verifications, one module per theme. Each function
//! here returns zero or more `CheckResult`s; `run_all` in this module
//! collects every family into the single ordered `Vec` `main` hands to
//! `report::print`.

pub mod build;

use crate::report::CheckResult;
use std::path::Path;

/// Runs every implemented check family against the repo at `root` and
/// returns all results, in catalog order (the report printer re-sorts by
/// group anyway, but keeping this in catalog order makes diffs of the raw
/// output stable).
pub fn run_all(root: &Path) -> Vec<CheckResult> {
    let mut results = Vec::new();
    results.extend(build::check_build_and_lock(root));
    results
}
