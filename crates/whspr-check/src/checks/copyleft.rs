//! Copyleft-by-name checks: dependency crate *names* that suggest a
//! GPL-family license, as a lighter complement to Z-08's SPDX-license-text
//! scan (`license.rs::check_no_copyleft_dependencies`). Split into its own
//! module to keep `license.rs` under the 600-line cap (AA-06).

use crate::repo;
use crate::report::CheckResult;
use std::path::Path;

/// Crate names (or name-shape patterns) that suggest a GPL-family license
/// by naming convention alone. Deliberately narrow: this is a
/// belt-and-suspenders check for dependencies whose manifest omits (or
/// mis-declares) a `license` field -- Z-08 already catches every
/// dependency that *does* declare a GPL-shaped SPDX license, so this only
/// adds value for the gap Z-08 can't see (below).
fn looks_copyleft_by_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "hyprland"
        || lower.starts_with("gpl-")
        || lower.ends_with("-gpl")
        || lower.contains("-gpl-")
}

/// Z-09: no GPL-family crate *names* among dependencies that have no
/// declared license for Z-08 to check.
///
/// Z-08 classifies every dependency that reports an SPDX license string in
/// `cargo metadata`; it has nothing to say about a dependency with no
/// `license`/`license_file` at all (Z-07 already flags that gap as
/// "missing metadata," not as copyleft). This check adds exactly the
/// signal Z-08 lacks: a name-shape denylist scoped to that undeclared-
/// license set, so a copyleft dependency can't hide from both Z-07 and
/// Z-08 simply by omitting its license metadata.
pub fn check_no_copyleft_crate_names(root: &Path) -> CheckResult {
    let meta = match repo::cargo_metadata(root) {
        Ok(m) => m,
        Err(e) => return CheckResult::fail("Z-09", e.to_string()),
    };
    let members: std::collections::HashSet<&str> = meta["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str())
        .collect();
    let packages = match meta["packages"].as_array() {
        Some(p) => p,
        None => return CheckResult::fail("Z-09", "cargo metadata JSON had no `packages` array"),
    };

    let suspects: Vec<&str> = packages
        .iter()
        .filter(|p| !members.contains(p["id"].as_str().unwrap_or_default()))
        .filter(|p| p["license"].as_str().is_none() && p["license_file"].as_str().is_none())
        .filter_map(|p| p["name"].as_str())
        .filter(|name| looks_copyleft_by_name(name))
        .collect();

    if suspects.is_empty() {
        CheckResult::pass(
            "Z-09",
            "no third-party dependency with an undeclared license has a GPL-suggestive crate \
             name (scoped to Z-07's undeclared-license set, complementing Z-08's SPDX-text scan)",
        )
    } else {
        CheckResult::fail(
            "Z-09",
            format!(
                "{} dependenc{} with no declared license {} a GPL-suggestive name: {}",
                suspects.len(),
                if suspects.len() == 1 { "y" } else { "ies" },
                if suspects.len() == 1 { "has" } else { "have" },
                suspects.join(", ")
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_copyleft_by_name_flags_hyprland_and_gpl_patterns() {
        assert!(looks_copyleft_by_name("hyprland"));
        assert!(looks_copyleft_by_name("HyprLand"));
        assert!(looks_copyleft_by_name("gpl-utils"));
        assert!(looks_copyleft_by_name("foo-gpl"));
        assert!(looks_copyleft_by_name("foo-gpl-bar"));
    }

    #[test]
    fn looks_copyleft_by_name_allows_permissive_looking_names() {
        assert!(!looks_copyleft_by_name("serde"));
        assert!(!looks_copyleft_by_name("tokio"));
        assert!(!looks_copyleft_by_name("applesque"));
    }
}
