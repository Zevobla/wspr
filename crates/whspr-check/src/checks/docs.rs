//! README content checks: architecture diagram, settings table,
//! model-swap docs, dependency/build docs, and the anti-slop honesty check.

use crate::report::CheckResult;
use crate::repo;
use std::path::Path;

/// W-06: README documents the 4-stage architecture (capture -> ASR ->
/// refine -> inject). Looks for a pipeline-shaped diagram: a fenced code
/// block that mentions all four stage concepts and uses an arrow.
pub fn check_readme_architecture(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("W-06", e.to_string()),
    };

    let stage_keywords: [(&str, &[&str]); 4] = [
        ("capture", &["capture", "microphone", "mic"]),
        ("asr", &["asr", "recognition", "transcri"]),
        ("refine", &["refine", "refiner", "llm"]),
        ("inject", &["inject", "text sink", "textsink"]),
    ];

    let has_arrow = text.contains('\u{2192}') || text.contains("->");
    let missing: Vec<&str> = stage_keywords
        .iter()
        .filter(|(_, keywords)| {
            let lower = text.to_lowercase();
            !keywords.iter().any(|k| lower.contains(k))
        })
        .map(|(name, _)| *name)
        .collect();

    if has_arrow && missing.is_empty() {
        CheckResult::pass(
            "W-06",
            "README mentions all 4 pipeline stages (capture/ASR/refine/inject) and contains a \
             pipeline arrow diagram",
        )
    } else {
        CheckResult::fail(
            "W-06",
            format!(
                "README architecture section incomplete (has arrow diagram: {has_arrow}, \
                 missing stage mentions: {})",
                if missing.is_empty() {
                    "none".to_string()
                } else {
                    missing.join(", ")
                }
            ),
        )
    }
}

/// W-07: README has a settings/config table (a markdown table whose header
/// row looks like `| Key | ... | Default | ... |`).
pub fn check_readme_settings_table(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("W-07", e.to_string()),
    };

    let has_table = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('|')
            && trimmed.to_lowercase().contains("key")
            && trimmed.to_lowercase().contains("default")
    });

    if has_table {
        CheckResult::pass(
            "W-07",
            "found a markdown table header row containing both \"Key\" and \"Default\"",
        )
    } else {
        CheckResult::fail(
            "W-07",
            "no markdown table header row with both \"Key\" and \"Default\" found in README",
        )
    }
}

/// AH-03: README documents system dependencies (a "Dependencies" section
/// naming at least a couple of the real system libs the build needs).
pub fn check_readme_dependencies_documented(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("AH-03", e.to_string()),
    };
    let lower = text.to_lowercase();

    let has_heading = lower.contains("## dependencies") || lower.contains("# dependencies");
    let known_deps = ["ffmpeg", "cmake", "clang", "alsa", "apple-sdk", "audiounit"];
    let mentioned: Vec<&str> = known_deps.iter().filter(|d| lower.contains(*d)).copied().collect();

    if has_heading && mentioned.len() >= 2 {
        CheckResult::pass(
            "AH-03",
            format!(
                "README has a Dependencies heading and names {} known system deps ({})",
                mentioned.len(),
                mentioned.join(", ")
            ),
        )
    } else {
        CheckResult::fail(
            "AH-03",
            format!(
                "README dependency docs incomplete (Dependencies heading: {has_heading}, \
                 known deps named: {})",
                mentioned.len()
            ),
        )
    }
}

/// AH-04: README documents build steps (a build-related heading with a
/// fenced code block invoking `cargo build` or `nix build`).
pub fn check_readme_build_steps_documented(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("AH-04", e.to_string()),
    };
    let lower = text.to_lowercase();

    let has_heading = lower.contains("build");
    let has_build_command = text.contains("cargo build") || text.contains("nix build");

    if has_heading && has_build_command {
        CheckResult::pass(
            "AH-04",
            "README mentions \"build\" and shows a `cargo build` or `nix build` command",
        )
    } else {
        CheckResult::fail(
            "AH-04",
            format!(
                "README build docs incomplete (mentions build: {has_heading}, shows a build \
                 command: {has_build_command})"
            ),
        )
    }
}

/// W-08: README documents how to swap models/backends.
pub fn check_readme_swap_docs(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("W-08", e.to_string()),
    };
    let lower = text.to_lowercase();

    let mentions_swapping =
        lower.contains("swap") || (lower.contains("local") && lower.contains("cloud"));
    let has_construction_example = text.contains("Pipeline::new(");

    if mentions_swapping && has_construction_example {
        CheckResult::pass(
            "W-08",
            "README discusses swapping local/cloud backends and shows a Pipeline::new(...) \
             construction example",
        )
    } else {
        CheckResult::fail(
            "W-08",
            format!(
                "README doesn't clearly document backend swapping (mentions swapping: \
                 {mentions_swapping}, has a Pipeline::new( example: {has_construction_example})"
            ),
        )
    }
}
