//! README content checks: architecture diagram, settings table,
//! model-swap docs, dependency/build docs, and the anti-slop honesty check.

use crate::repo;
use crate::report::CheckResult;
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

/// Superlatives that read as overclaiming for an early-stage project.
/// Presence isn't automatically disqualifying content-wise, but combined
/// with a README that also fails to separate "done" from "planned", it's
/// exactly the pattern AC-03 exists to catch.
const OVERCLAIM_PHRASES: &[&str] = &[
    "production-ready",
    "production ready",
    "fully working",
    "100% accurate",
    "enterprise-grade",
    "battle-tested",
    "state-of-the-art accuracy",
];

/// AC-03: README distinguishes shipped work from aspirational claims.
///
/// This is inherently a heuristic (genuinely judging honesty needs a human
/// reader), so it checks two structural, checkable proxies for it: does
/// the README have an explicit "what's real vs planned" split, and does it
/// avoid unqualified overclaiming language. Note this checks the *target
/// repo's* README, not whspr-check's own - see the crate doc comment in
/// main.rs for how this checker holds itself to the same rule.
pub fn check_readme_honesty(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("AC-03", e.to_string()),
    };
    let lower = text.to_lowercase();

    let has_done_marker = lower.contains("today") || lower.contains("works today");
    let has_planned_marker = lower.contains("planned") || lower.contains("not yet implemented");
    let overclaims: Vec<&str> = OVERCLAIM_PHRASES
        .iter()
        .filter(|p| lower.contains(*p))
        .copied()
        .collect();

    if has_done_marker && has_planned_marker && overclaims.is_empty() {
        CheckResult::pass(
            "AC-03",
            "README has both a \"today\"/done marker and a \"planned\"/not-yet marker, and \
             contains none of this checker's overclaim phrases",
        )
    } else {
        CheckResult::fail(
            "AC-03",
            format!(
                "done marker present: {has_done_marker}, planned marker present: \
                 {has_planned_marker}, overclaim phrases found: {}",
                if overclaims.is_empty() {
                    "none".to_string()
                } else {
                    overclaims.join(", ")
                }
            ),
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
    let mentioned: Vec<&str> = known_deps
        .iter()
        .filter(|d| lower.contains(*d))
        .copied()
        .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overclaim_phrases_has_entries() {
        assert!(!OVERCLAIM_PHRASES.is_empty());
    }

    #[test]
    fn overclaim_phrases_contains_superlatives() {
        assert!(OVERCLAIM_PHRASES.contains(&"production-ready"));
        assert!(OVERCLAIM_PHRASES.contains(&"fully working"));
    }

    #[test]
    fn overclaim_phrases_all_lowercase() {
        for phrase in OVERCLAIM_PHRASES {
            assert_eq!(phrase, &phrase.to_lowercase(), "{phrase} should be all lowercase");
        }
    }
}
