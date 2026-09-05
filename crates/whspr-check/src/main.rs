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

fn main() -> anyhow::Result<()> {
    println!("whspr-check: scaffold");
    Ok(())
}
