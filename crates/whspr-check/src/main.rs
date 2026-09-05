//! `whspr-check`: an independent, automated acceptance checker for the whspr
//! workspace. Run from within the repo (any directory under it) via
//! `cargo run -p whspr-check`. It inspects the repo tree, git history, and
//! (by shelling out to `cargo`) the build/test/lint state, then prints a
//! scored report against a curated subset of the 574-criterion acceptance
//! matrix.
//!
//! Honesty rule this crate follows (mirrors criterion AC-03): a criterion is
//! only ever reported PASS when this code actually verified it. Anything
//! this checker doesn't implement a concrete verification for is left out of
//! the catalog entirely and counted in the report's "not yet automated"
//! bucket — never guessed at or marked PASS on faith.

mod criteria;
mod report;
mod repo;

use report::CheckResult;

fn main() -> anyhow::Result<()> {
    let root = repo::find_repo_root()?;

    // Placeholder result until the real checks land; proves `report` wires
    // into `main` and into the catalog.
    let placeholder = CheckResult::needs_bench(
        criteria::CATALOG[0].id,
        "checker not implemented yet".to_string(),
    );
    println!(
        "whspr-check: repo root = {}; {} criteria in catalog (of {} total in the acceptance matrix); e.g. {} -> {:?}",
        root.display(),
        criteria::CATALOG.len(),
        criteria::TOTAL_CRITERIA,
        placeholder.id,
        placeholder.verdict
    );
    Ok(())
}
