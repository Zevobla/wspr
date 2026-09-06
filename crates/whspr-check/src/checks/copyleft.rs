//! Copyleft dependency checks: GPL crate names in the resolved dependency
//! tree. This is split from license.rs to keep both files under the 600-line
//! limit (AA-06).

use crate::report::CheckResult;
use std::path::Path;

/// Checks a crate name against a denylist of copyleft-indicative patterns.
/// Returns true if the name matches the denylist (should be denied).
fn is_copyleft_crate_name(name: &str) -> bool {
    // Exact denylist: hyprland and crates with GPL-indicative names
    if name == "hyprland" {
        return true;
    }
    // Match "*-gpl*" pattern (e.g., "libgpl", "rust-gpl")
    if name.contains("-gpl") || name.ends_with("gpl") || name.starts_with("gpl-") {
        return true;
    }
    false
}

/// Z-09: no GPL-family crate names in the resolved dependency tree.
///
/// Scans Cargo.lock for crate names that suggest copyleft licenses
/// (hyprland, *-gpl patterns) - a lighter check than examining actual license
/// metadata, but catches low-hanging fruit without needing to parse SPDX
/// expressions. This is a guard against accidentally depending on known
/// copyleft projects.
pub fn check_no_copyleft_crates(root: &Path) -> CheckResult {
    let lock_path = root.join("Cargo.lock");
    let lock_content = match std::fs::read_to_string(&lock_path) {
        Ok(c) => c,
        Err(e) => {
            return CheckResult::fail(
                "Z-09",
                format!("could not read Cargo.lock: {e}"),
            )
        }
    };

    let mut copyleft_crates: Vec<String> = Vec::new();
    for line in lock_content.lines() {
        // Cargo.lock format: [[package]] followed by name = "..."
        if line.starts_with("name = ") {
            if let Some(name) = line.strip_prefix("name = \"").and_then(|s| s.strip_suffix("\"")) {
                if is_copyleft_crate_name(name) {
                    copyleft_crates.push(name.to_string());
                }
            }
        }
    }

    if copyleft_crates.is_empty() {
        CheckResult::pass(
            "Z-09",
            "Cargo.lock contains no crates matching copyleft denylist patterns (hyprland, \
             *-gpl, ...)",
        )
    } else {
        // Deduplicate if a crate appears multiple times
        copyleft_crates.sort();
        copyleft_crates.dedup();
        CheckResult::fail(
            "Z-09",
            format!(
                "{} copyleft-pattern crate name(s) found in Cargo.lock: {}",
                copyleft_crates.len(),
                copyleft_crates.join(", ")
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_copyleft_crate_name_detects_hyprland() {
        assert!(is_copyleft_crate_name("hyprland"));
    }

    #[test]
    fn is_copyleft_crate_name_detects_gpl_patterns() {
        assert!(is_copyleft_crate_name("libgpl"));
        assert!(is_copyleft_crate_name("rust-gpl"));
        assert!(is_copyleft_crate_name("gpl-3"));
        assert!(is_copyleft_crate_name("my-gpl-lib"));
    }

    #[test]
    fn is_copyleft_crate_name_allows_permissive() {
        assert!(!is_copyleft_crate_name("lib-mit"));
        assert!(!is_copyleft_crate_name("apache2"));
        assert!(!is_copyleft_crate_name("serde"));
        assert!(!is_copyleft_crate_name("tokio"));
    }
}
