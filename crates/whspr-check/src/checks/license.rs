//! License and provenance checks: LICENSE file presence, the declared
//! SPDX id, whether it's named up front in the README, and whether model
//! weights/secrets ever made it into the tracked tree or git history.

use crate::report::CheckResult;
use crate::repo;
use crate::util::MODEL_WEIGHT_EXTENSIONS;
use std::path::Path;

/// Z-01: LICENSE file present at repo root.
pub fn check_license_file_present(root: &Path) -> CheckResult {
    let path = root.join("LICENSE");
    if path.is_file() {
        CheckResult::pass("Z-01", format!("{} exists", path.display()))
    } else {
        CheckResult::fail("Z-01", format!("{} does not exist", path.display()))
    }
}

/// A conservative allow-list of common OSI-approved / well-known SPDX
/// license identifiers. Not exhaustive (SPDX has hundreds) - just enough
/// to recognize the licenses a project like this would plausibly use, so
/// Z-02 doesn't have to bundle the full SPDX license list as a dependency.
const KNOWN_SPDX_IDS: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MPL-2.0",
    "Unlicense",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
];

/// Reads `workspace.package.license` out of the root Cargo.toml.
fn declared_license(root: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let value: toml::Value = toml::from_str(&text)?;
    value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("license"))
        .and_then(|l| l.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no workspace.package.license key in Cargo.toml"))
}

/// Z-02: declared license is a recognized SPDX/OSI identifier. A-04:
/// declared license is specifically MIT or Apache-2.0.
///
/// Both read the same `workspace.package.license` value, so they're one
/// function returning both verdicts rather than parsing Cargo.toml twice.
pub fn check_declared_license(root: &Path) -> Vec<CheckResult> {
    let license = match declared_license(root) {
        Ok(l) => l,
        Err(e) => {
            return vec![
                CheckResult::fail("Z-02", format!("could not read declared license: {e}")),
                CheckResult::fail("A-04", format!("could not read declared license: {e}")),
            ]
        }
    };

    let z02 = if KNOWN_SPDX_IDS.contains(&license.as_str()) {
        CheckResult::pass(
            "Z-02",
            format!("workspace.package.license = \"{license}\" is a recognized SPDX id"),
        )
    } else {
        CheckResult::fail(
            "Z-02",
            format!(
                "workspace.package.license = \"{license}\" is not in this checker's known-SPDX \
                 allow-list ({} ids checked)",
                KNOWN_SPDX_IDS.len()
            ),
        )
    };

    let a04 = if license == "MIT" || license == "Apache-2.0" {
        CheckResult::pass("A-04", format!("declared license is \"{license}\""))
    } else {
        CheckResult::fail(
            "A-04",
            format!(
                "declared license is \"{license}\", not MIT or Apache-2.0 (whspr is AGPL-3.0-or-later \
                 by deliberate project choice - this is an honest FAIL against this specific \
                 criterion, not a bug)"
            ),
        )
    };

    vec![z02, a04]
}

/// Z-12: model weights are gitignored and absent from the tracked tree.
/// Both halves matter: a .gitignore entry alone doesn't prove nothing was
/// committed before the ignore rule was added, and no tracked files alone
/// doesn't prove a contributor won't accidentally commit one next time.
pub fn check_model_weights_ignored_and_absent(root: &Path) -> CheckResult {
    let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let ignored_patterns: Vec<&str> = MODEL_WEIGHT_EXTENSIONS
        .iter()
        .filter(|ext| gitignore.contains(&format!("*{ext}")))
        .copied()
        .collect();

    let tracked = match repo::git_ls_files(root) {
        Ok(files) => files,
        Err(e) => return CheckResult::fail("Z-12", format!("could not list tracked files: {e}")),
    };
    let tracked_model_files: Vec<String> = tracked
        .into_iter()
        .filter(|p| {
            let lower = p.to_lowercase();
            MODEL_WEIGHT_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
        })
        .collect();

    if ignored_patterns.len() == MODEL_WEIGHT_EXTENSIONS.len() && tracked_model_files.is_empty() {
        CheckResult::pass(
            "Z-12",
            format!(
                ".gitignore covers all {} model-weight extensions ({}) and 0 are tracked",
                MODEL_WEIGHT_EXTENSIONS.len(),
                MODEL_WEIGHT_EXTENSIONS.join(", ")
            ),
        )
    } else {
        CheckResult::fail(
            "Z-12",
            format!(
                ".gitignore covers {}/{} model-weight extensions ({}); tracked model-weight \
                 files: {}",
                ignored_patterns.len(),
                MODEL_WEIGHT_EXTENSIONS.len(),
                ignored_patterns.join(", "),
                if tracked_model_files.is_empty() {
                    "none".to_string()
                } else {
                    tracked_model_files.join(", ")
                }
            ),
        )
    }
}

/// Prefixes/markers distinctive enough to a real secret that they're worth
/// flagging, without being so generic (a bare "sk-", "key =", ...) that
/// they'd drown in false positives. Not exhaustive - a pattern-based scan
/// like this one is a heuristic tripwire, not a guarantee.
const SECRET_PATTERNS: &[&str] = &[
    "AKIA",     // AWS access key id
    "ghp_", "gho_", "ghu_", "ghs_", "ghr_", // GitHub tokens
    "sk-proj-", "sk-ant-", // OpenAI project / Anthropic key prefixes
    "AIzaSy",   // Google API key
    "xoxb-", "xoxp-", "xoxa-", // Slack tokens
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "-----BEGIN EC PRIVATE KEY-----",
    "-----BEGIN PGP PRIVATE KEY BLOCK-----",
];

/// If a matched line also contains one of these (case-insensitive), it's
/// almost certainly a test fixture rather than a real credential (e.g.
/// whspr-config's own tests write `openai = "sk-test-123"` to a temp
/// config file) - skip it rather than cry wolf.
const BENIGN_MARKERS: &[&str] = &[
    "test", "example", "dummy", "fake", "xxx", "placeholder", "redacted", "sample",
];

/// Z-16: no obvious secrets/API keys committed anywhere in git history.
///
/// Scans `git log --all -p` (every added line, across every ref, not just
/// the current branch) for the patterns above. This is a pattern-based
/// heuristic, not an exhaustive secret scanner (that's a whole product
/// category on its own) - documented as such in the evidence either way.
pub fn check_no_secrets_in_history(root: &Path) -> CheckResult {
    let output = match repo::run(root, "git", &["log", "--all", "-p"]) {
        Ok(o) => o,
        Err(e) => return CheckResult::fail("Z-16", format!("could not run git log: {e}")),
    };
    if !output.success {
        return CheckResult::fail(
            "Z-16",
            format!("`git log --all -p` failed: {}", output.stderr),
        );
    }

    let mut current_commit = "?";
    let mut findings: Vec<String> = Vec::new();
    for line in output.stdout.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            current_commit = hash.get(..10).unwrap_or(hash);
            continue;
        }
        // Added lines only (skip the "+++ b/path" diff header, which also
        // starts with '+').
        if !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let lower = line.to_lowercase();
        if BENIGN_MARKERS.iter().any(|m| lower.contains(m)) {
            continue;
        }
        if let Some(pattern) = SECRET_PATTERNS.iter().find(|p| line.contains(**p)) {
            findings.push(format!(
                "{current_commit}: matched \"{pattern}\" in: {}",
                crate::util::head(line.trim(), 120)
            ));
        }
    }

    if findings.is_empty() {
        CheckResult::pass(
            "Z-16",
            format!(
                "0 matches across {} known secret-prefix patterns in `git log --all -p` \
                 (heuristic scan, not exhaustive)",
                SECRET_PATTERNS.len()
            ),
        )
    } else {
        CheckResult::fail(
            "Z-16",
            format!("possible secret(s) found in git history: {}", findings.join(" | ")),
        )
    }
}

/// How many lines of the README count as its "first screen" for Z-04 - a
/// generous approximation of what's visible without scrolling on GitHub's
/// rendered view.
const README_FIRST_SCREEN_LINES: usize = 20;

/// Z-04: license is named on the README's first screen.
pub fn check_readme_names_license(root: &Path) -> CheckResult {
    let text = match repo::read_readme(root) {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("Z-04", e.to_string()),
    };

    let first_screen: String = text
        .lines()
        .take(README_FIRST_SCREEN_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let lower = first_screen.to_lowercase();

    let mentions_license_word = lower.contains("license");
    let mentions_a_license_id = KNOWN_SPDX_IDS
        .iter()
        .any(|id| lower.contains(&id.to_lowercase()))
        || lower.contains("agpl"); // AGPL-3.0 (short form) isn't in KNOWN_SPDX_IDS verbatim

    if mentions_license_word && mentions_a_license_id {
        CheckResult::pass(
            "Z-04",
            format!(
                "README's first {README_FIRST_SCREEN_LINES} lines mention \"license\" and a \
                 license id"
            ),
        )
    } else {
        CheckResult::fail(
            "Z-04",
            format!(
                "README's first {README_FIRST_SCREEN_LINES} lines don't clearly name a license \
                 (mentions \"license\": {mentions_license_word}, mentions a known license id: \
                 {mentions_a_license_id})"
            ),
        )
    }
}
