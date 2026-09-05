//! AA: architecture/code-quality checks that don't fit cleanly under any
//! other theme's file - crate dependency shape (from `cargo metadata`),
//! source-file size sanity, and logging discipline.

use crate::repo;
use crate::report::CheckResult;
use std::path::Path;

/// AA-06: module/file size sanity - no single tracked `*.rs` file has grown
/// unreasonably large.
///
/// 600 lines is this checker's own defensible-but-not-official threshold
/// (matching the style of the AD-group checks' self-declared heuristics):
/// large enough that every file in this repo comfortably fits under it
/// today (the biggest, `whspr-refine/src/lib.rs`, is 504 lines), small
/// enough to still mean something - a file past this is a real candidate
/// for splitting.
pub fn check_file_size_sanity(root: &Path) -> CheckResult {
    let files = match repo::git_ls_files(root) {
        Ok(f) => f,
        Err(e) => return CheckResult::fail("AA-06", format!("could not list tracked files: {e}")),
    };
    let rs_files: Vec<&String> = files.iter().filter(|f| f.ends_with(".rs")).collect();

    const MAX_LINES: usize = 600;
    let mut oversized: Vec<(String, usize)> = Vec::new();
    let mut max_seen = 0usize;
    for f in &rs_files {
        let Ok(contents) = std::fs::read_to_string(root.join(f)) else {
            continue; // tracked but unreadable (e.g. deleted in the working tree) - not this check's concern
        };
        let lines = contents.lines().count();
        max_seen = max_seen.max(lines);
        if lines > MAX_LINES {
            oversized.push(((*f).clone(), lines));
        }
    }

    if oversized.is_empty() {
        CheckResult::pass(
            "AA-06",
            format!(
                "largest of {} tracked *.rs files is {max_seen} lines (threshold: <= \
                 {MAX_LINES})",
                rs_files.len()
            ),
        )
    } else {
        CheckResult::fail(
            "AA-06",
            format!(
                "{} file(s) exceed the {MAX_LINES}-line threshold: {}",
                oversized.len(),
                oversized
                    .iter()
                    .map(|(f, l)| format!("{f} ({l} lines)"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    }
}
