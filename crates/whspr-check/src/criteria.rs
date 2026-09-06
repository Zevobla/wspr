//! The criteria catalog: static metadata for every criterion this checker
//! knows how to verify. This module holds *only* data — id, group, and a
//! human-readable title — no verification logic. The check functions in
//! `crate::checks` reference these ids when they emit a `CheckResult`; the
//! report formatter looks titles/groups up from here.
//!
//! The full acceptance matrix has 574 criteria across the 38 groups listed
//! in `GROUPS` below. This checker does not attempt all 574: `CATALOG` is a
//! deliberately curated, concretely machine-checkable subset. Anything not
//! listed here is out of scope for this tool today (counted in the report
//! as "not yet automated," not as a failure) rather than guessed at.

/// One entry in the acceptance matrix's 38 top-level groups: a letter code
/// (`A`..`AL`) and its short name, as given in the acceptance-matrix brief.
/// This is reference metadata for the report header only; we don't have a
/// per-group breakdown of how many of the 574 criteria live in each group.
pub const GROUPS: &[(&str, &str)] = &[
    ("A", "Build/install"),
    ("B", "First-run/config"),
    ("C", "Mic/audio devices"),
    ("D", "Hotkeys/modes"),
    ("E", "Speech capture/VAD"),
    ("F", "Recognition accuracy"),
    ("G", "Punctuation/case/structure"),
    ("H", "Term dictionary"),
    ("I", "Mixed speech/languages"),
    ("J", "Post-processing/style"),
    ("K", "Text insertion"),
    ("L", "Clipboard"),
    ("M", "Utterance history"),
    ("N", "Audio-file upload"),
    ("O", "Fail-safe/recovery"),
    ("P", "Autonomy/privacy"),
    ("Q", "Latency"),
    ("R", "Compute resource"),
    ("S", "Tokens/cost"),
    ("T", "Stats/journal"),
    ("U", "Long-run"),
    ("V", "Security"),
    ("W", "UI/docs"),
    ("X", "Cross-platform parity"),
    ("Y", "CLI/batch"),
    ("Z", "Licenses/provenance"),
    ("AA", "Architecture/code-quality"),
    ("AB", "Tests"),
    ("AC", "Slop-meter"),
    ("AD", "Growth-monotonicity(git)"),
    ("AE", "Uniqueness"),
    ("AF", "Load/server"),
    ("AG", "Mic/lock"),
    ("AH", "Install/update/uninstall"),
    ("AI", "Noise/voice/adaptation"),
    ("AJ", "Macros/feedback"),
    ("AK", "Original-feature"),
    ("AL", "Input-speed/stats"),
];

/// Total number of criteria in the full acceptance matrix (given, not
/// derived) — the denominator the report's coverage line is measured
/// against.
pub const TOTAL_CRITERIA: usize = 574;

/// A single criterion this checker can verify.
pub struct Criterion {
    pub id: &'static str,
    pub group: &'static str,
    pub title: &'static str,
}

/// Looks up catalog metadata for a criterion id. Panics on an unknown id —
/// that's a bug in a check (it emitted a `CheckResult` for an id that was
/// never registered here), and should fail loudly in development rather
/// than silently print a blank title.
pub fn lookup(id: &str) -> &'static Criterion {
    CATALOG
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("whspr-check bug: criterion id {id:?} has no catalog entry"))
}

pub const CATALOG: &[Criterion] = &[
    // --- A: Build/install ---
    Criterion {
        id: "A-01",
        group: "A",
        title: "One-command build works (cargo build --workspace)",
    },
    Criterion {
        id: "A-03",
        group: "A",
        title: "Cargo.lock is present and committed",
    },
    Criterion {
        id: "A-04",
        group: "A",
        title: "License is MIT or Apache-2.0",
    },
    Criterion {
        id: "A-13",
        group: "A",
        title: "Full test suite passes with a single command",
    },
    // --- AA: Architecture/code-quality ---
    Criterion {
        id: "AA-13",
        group: "AA",
        title: "clippy -D warnings is clean across the workspace",
    },
    Criterion {
        id: "AA-14",
        group: "AA",
        title: "cargo fmt --check is clean across the workspace",
    },
    Criterion {
        id: "AA-06",
        group: "AA",
        title: "No single source file has grown unreasonably large",
    },
    Criterion {
        id: "AA-16",
        group: "AA",
        title: "Logging goes through one interface, not scattered println!/eprintln!",
    },
    Criterion {
        id: "AA-02",
        group: "AA",
        title: "Domain crate (whspr-core) is free of heavy framework deps",
    },
    Criterion {
        id: "AA-10",
        group: "AA",
        title: "No circular dependencies between workspace crates",
    },
    // --- Z: Licenses/provenance ---
    Criterion {
        id: "Z-01",
        group: "Z",
        title: "LICENSE file present at repo root",
    },
    Criterion {
        id: "Z-02",
        group: "Z",
        title: "Declared license is a recognized SPDX/OSI identifier",
    },
    Criterion {
        id: "Z-03",
        group: "Z",
        title: "LICENSE file has filled copyright notice (not placeholder)",
    },
    Criterion {
        id: "Z-04",
        group: "Z",
        title: "License is named on the README's first screen",
    },
    Criterion {
        id: "Z-07",
        group: "Z",
        title: "Dependency-license inventory: every resolved dep reports a license",
    },
    Criterion {
        id: "Z-08",
        group: "Z",
        title: "No copyleft dependency licenses under our permissive license",
    },
    Criterion {
        id: "Z-09",
        group: "Z",
        title: "No GPL-family crate names in resolved dependency tree",
    },
    Criterion {
        id: "Z-12",
        group: "Z",
        title: "Model weights are gitignored and absent from the tracked tree",
    },
    Criterion {
        id: "Z-16",
        group: "Z",
        title: "No obvious secrets/API keys committed anywhere in git history",
    },
    // --- W: UI/docs ---
    Criterion {
        id: "W-06",
        group: "W",
        title: "README documents the 4-stage architecture",
    },
    Criterion {
        id: "W-07",
        group: "W",
        title: "README has a settings/config table",
    },
    Criterion {
        id: "W-08",
        group: "W",
        title: "README documents how to swap models/backends",
    },
    // --- AH: Install/update/uninstall ---
    Criterion {
        id: "AH-03",
        group: "AH",
        title: "README documents system dependencies",
    },
    Criterion {
        id: "AH-04",
        group: "AH",
        title: "README documents build steps",
    },
    // --- AB: Tests ---
    Criterion {
        id: "AB-06",
        group: "AB",
        title: "Unit tests run without a mic or model files present",
    },
    Criterion {
        id: "AB-13",
        group: "AB",
        title: "Tests are isolated from the network",
    },
    Criterion {
        id: "AB-15",
        group: "AB",
        title: "Tests run in CI",
    },
    Criterion {
        id: "AB-05",
        group: "AB",
        title: "Test-suite runtime is bounded",
    },
    Criterion {
        id: "AB-12",
        group: "AB",
        title: "Tests give identical outcomes across repeat runs",
    },
    // --- AD: Growth-monotonicity(git) ---
    Criterion {
        id: "AD-01",
        group: "AD",
        title: "Commit count reflects incremental history",
    },
    Criterion {
        id: "AD-02",
        group: "AD",
        title: "Median inter-commit interval indicates organic (non-scripted) pacing",
    },
    Criterion {
        id: "AD-03",
        group: "AD",
        title: "Max gap between commits is bounded",
    },
    Criterion {
        id: "AD-04",
        group: "AD",
        title: "No single commit dominates the source-code change volume",
    },
    Criterion {
        id: "AD-07",
        group: "AD",
        title: "Tests land alongside the code they cover, not in one bulk commit",
    },
    Criterion {
        id: "AD-11",
        group: "AD",
        title: "Commit authorship is a single consistent identity",
    },
    Criterion {
        id: "AD-06",
        group: "AD",
        title: "Commits are distributed over time, not clustered into one burst",
    },
    // --- Y: CLI/batch ---
    Criterion {
        id: "Y-03",
        group: "Y",
        title: "whspr --version prints a version string",
    },
    Criterion {
        id: "Y-04",
        group: "Y",
        title: "CLI exits non-zero on error",
    },
    Criterion {
        id: "Y-11",
        group: "Y",
        title: "whspr --version exits zero",
    },
    Criterion {
        id: "Y-12",
        group: "Y",
        title: "whspr --help works",
    },
    Criterion {
        id: "Y-15",
        group: "Y",
        title: "Progress/log output goes to stderr, not stdout",
    },
    Criterion {
        id: "Y-13",
        group: "Y",
        title: "CLI works headless (no display-server env vars present)",
    },
    Criterion {
        id: "Y-14",
        group: "Y",
        title: "Repeat run gives identical output (determinism)",
    },
    // --- P: Autonomy/privacy ---
    Criterion {
        id: "P-01",
        group: "P",
        title: "transcribe runs with no network reachability for the mock/local path",
    },
    Criterion {
        id: "P-08",
        group: "P",
        title: "Default backend selection is local/offline, not cloud",
    },
    // --- B: First-run/config ---
    Criterion {
        id: "B-03",
        group: "B",
        title: "Config file is created on first run",
    },
    Criterion {
        id: "B-04",
        group: "B",
        title: "Config file format is TOML",
    },
    Criterion {
        id: "B-05",
        group: "B",
        title: "Default config contains all required sections/keys",
    },
    Criterion {
        id: "B-14",
        group: "B",
        title: "Config lives in the platform config directory",
    },
    // --- AC: Slop-meter ---
    Criterion {
        id: "AC-02",
        group: "AC",
        title: "Low count of empty/stub (todo!/unimplemented!) function bodies",
    },
    Criterion {
        id: "AC-03",
        group: "AC",
        title: "README distinguishes shipped work from aspirational claims",
    },
    Criterion {
        id: "AC-07",
        group: "AC",
        title: "No unused workspace dependencies (cargo-udeps)",
    },
    Criterion {
        id: "AC-06",
        group: "AC",
        title: "Dead code absent (clippy -D warnings, dead_code lint)",
    },
    // --- AE: Uniqueness ---
    Criterion {
        id: "AE-10",
        group: "AE",
        title: "UNIQUENESS.md file is present at repo root",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_has_expected_entries() {
        assert!(!GROUPS.is_empty());
        assert_eq!(GROUPS.len(), 38);
    }

    #[test]
    fn groups_first_entry_is_a_group() {
        assert_eq!(GROUPS[0].0, "A");
        assert!(!GROUPS[0].1.is_empty());
    }

    #[test]
    fn lookup_finds_known_criterion() {
        let criterion = lookup("A-01");
        assert_eq!(criterion.id, "A-01");
        assert_eq!(criterion.group, "A");
    }

    #[test]
    #[should_panic(expected = "whspr-check bug")]
    fn lookup_panics_on_unknown_criterion() {
        lookup("UNKNOWN-99");
    }

    #[test]
    fn catalog_has_non_zero_entries() {
        assert!(!CATALOG.is_empty());
    }

    #[test]
    fn total_criteria_matches_acceptance_matrix() {
        assert_eq!(TOTAL_CRITERIA, 574);
    }
}
