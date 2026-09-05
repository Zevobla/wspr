//! AA: architecture/code-quality checks that don't fit cleanly under any
//! other theme's file - crate dependency shape (from `cargo metadata`),
//! source-file size sanity, and logging discipline.

use crate::repo;
use crate::report::CheckResult;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Runs `cargo metadata --format-version 1` and parses its JSON, shared by
/// every check in this file that needs the resolved dependency graph
/// (AA-02, AA-10) rather than just reading Cargo.toml files directly.
fn run_cargo_metadata(root: &Path) -> anyhow::Result<Value> {
    let output = repo::run(root, "cargo", &["metadata", "--format-version", "1"])?;
    if !output.success {
        anyhow::bail!("`cargo metadata` failed: {}", output.stderr);
    }
    serde_json::from_str(&output.stdout)
        .map_err(|e| anyhow::anyhow!("could not parse cargo metadata JSON: {e}"))
}

/// Crates that make `whspr-core` no longer "light": native-compiling ASR/
/// LLM backends, the GUI toolkit, and OS-integration libraries. Per
/// CLAUDE.md, whspr-core is supposed to depend on nothing else in the
/// workspace and only light deps (async-trait, thiserror, serde, tokio,
/// tracing) - this check verifies that claim against the real, resolved
/// manifest instead of trusting the doc comment.
const HEAVY_DEPS: &[&str] = &[
    "whisper-rs",
    "llama-cpp-2",
    "iced",
    "cpal",
    "global-hotkey",
    "enigo",
    "arboard",
    "reqwest",
];

/// AA-02: the domain crate (whspr-core) is free of heavy framework deps.
pub fn check_core_free_of_heavy_deps(root: &Path) -> CheckResult {
    let meta = match run_cargo_metadata(root) {
        Ok(m) => m,
        Err(e) => return CheckResult::fail("AA-02", e.to_string()),
    };
    let Some(packages) = meta["packages"].as_array() else {
        return CheckResult::fail("AA-02", "cargo metadata JSON had no `packages` array");
    };
    let Some(core) = packages.iter().find(|p| p["name"] == "whspr-core") else {
        return CheckResult::fail("AA-02", "no whspr-core package in cargo metadata output");
    };
    let Some(deps) = core["dependencies"].as_array() else {
        return CheckResult::fail("AA-02", "whspr-core package has no `dependencies` array");
    };

    let normal_deps: Vec<&str> = deps
        .iter()
        .filter(|d| d["kind"].is_null()) // normal deps only, not dev/build
        .filter_map(|d| d["name"].as_str())
        .collect();
    let heavy_found: Vec<&str> = normal_deps
        .iter()
        .filter(|d| HEAVY_DEPS.contains(d))
        .copied()
        .collect();

    if heavy_found.is_empty() {
        CheckResult::pass(
            "AA-02",
            format!(
                "whspr-core's declared normal dependencies are: {} - none of the {} known \
                 heavy/framework deps ({}) are among them",
                normal_deps.join(", "),
                HEAVY_DEPS.len(),
                HEAVY_DEPS.join(", ")
            ),
        )
    } else {
        CheckResult::fail(
            "AA-02",
            format!(
                "whspr-core directly depends on heavy/framework crate(s): {}",
                heavy_found.join(", ")
            ),
        )
    }
}

/// The library crates AA-16 holds to a no-direct-printing standard.
/// Deliberately excludes the binary crates (`whspr-cli`, `whspr-app`),
/// which legitimately own their own stdout for direct user-facing program
/// output, and `whspr-check` itself, which prints its report.
const LIBRARY_CRATES: &[&str] = &[
    "whspr-core",
    "whspr-asr",
    "whspr-refine",
    "whspr-audio",
    "whspr-config",
    "whspr-inject",
];

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

/// AA-16: logging goes through one interface, not scattered
/// `println!`/`eprintln!` calls.
///
/// Scoped to `LIBRARY_CRATES` only, and excludes hits inside a `#[cfg(test)]`
/// module: every test module in this repo is a trailing block at the end of
/// its file (verified by inspection, not assumed), so "does this hit's line
/// number come before the file's own `#[cfg(test)]` line" precisely
/// separates real code from conventional test-diagnostic `eprintln!`s,
/// without needing an actual Rust parser.
pub fn check_logging_single_interface(root: &Path) -> CheckResult {
    let pathspecs: Vec<String> = LIBRARY_CRATES
        .iter()
        .map(|c| format!("crates/{c}/src"))
        .collect();
    let pathspec_refs: Vec<&str> = pathspecs.iter().map(String::as_str).collect();

    let matches = match repo::git_grep(root, &["-E"], r"(println|eprintln)!\(", &pathspec_refs) {
        Ok(m) => m,
        Err(e) => {
            return CheckResult::fail(
                "AA-16",
                format!("could not grep for println!/eprintln!: {e}"),
            )
        }
    };

    let mut test_marker_line_by_path: HashMap<String, usize> = HashMap::new();
    let mut real_hits: Vec<String> = Vec::new();

    for line in &matches {
        let mut parts = line.splitn(3, ':');
        let (Some(path), Some(lineno_str), Some(content)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if content.trim_start().starts_with("//") {
            continue; // mentioned in a comment, not a real call site
        }
        let Ok(lineno) = lineno_str.parse::<usize>() else {
            continue;
        };

        let test_line = *test_marker_line_by_path
            .entry(path.to_string())
            .or_insert_with(|| {
                repo::git_grep(root, &["-F", "-m", "1"], "#[cfg(test)]", &[path])
                    .ok()
                    .and_then(|m| m.first()?.split(':').nth(1)?.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            });

        if lineno < test_line {
            real_hits.push(line.clone());
        }
    }

    if real_hits.is_empty() {
        CheckResult::pass(
            "AA-16",
            format!(
                "no println!/eprintln! outside #[cfg(test)] sections across the {} library \
                 crates ({})",
                LIBRARY_CRATES.len(),
                LIBRARY_CRATES.join(", ")
            ),
        )
    } else {
        CheckResult::fail(
            "AA-16",
            format!(
                "{} println!/eprintln! call site(s) in library crates, outside test code \
                 (should go through a shared logging facade like `tracing`, which is a declared \
                 workspace dependency but has zero actual macro usage anywhere in the workspace \
                 today): {}",
                real_hits.len(),
                real_hits.join("; ")
            ),
        )
    }
}
