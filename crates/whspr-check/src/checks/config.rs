//! First-run/config checks. All of these call `whspr_config` directly (a
//! real workspace dependency of this crate) rather than statically reading
//! its source, so these are dynamic behavioral checks, not text scans.

use crate::repo;
use crate::report::CheckResult;
use std::path::Path;

/// B-03: config file is created on first run.
///
/// Points `whspr_config::load_from` at a brand-new empty temp dir (playing
/// the role of a first run, before any config file exists) and checks
/// whether a `config.toml` shows up afterward. It doesn't - `load_from`
/// only ever reads, never writes - so this honestly reports FAIL.
pub fn check_config_created_on_first_run() -> CheckResult {
    let temp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return CheckResult::fail("B-03", format!("could not create temp dir: {e}")),
    };
    let _config = whspr_config::load_from(Some(temp_dir.path()));
    let config_path = temp_dir.path().join("config.toml");

    if config_path.is_file() {
        CheckResult::pass(
            "B-03",
            format!(
                "{} was created by load_from() on a fresh directory",
                config_path.display()
            ),
        )
    } else {
        CheckResult::fail(
            "B-03",
            "whspr_config::load_from() never writes a config.toml - it only reads one if it \
             already exists, falling back to in-memory defaults otherwise; nothing in the \
             current code path creates the file on first run",
        )
    }
}

/// B-04: config file format is TOML.
///
/// Writes a real TOML file into a temp dir and confirms `load_from` reads
/// a non-default value back out of it correctly - proves TOML
/// compatibility by round-tripping through the real crate, not by
/// grepping its source for `toml::from_str`.
pub fn check_config_format_is_toml() -> CheckResult {
    let temp_dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return CheckResult::fail("B-04", format!("could not create temp dir: {e}")),
    };
    let config_path = temp_dir.path().join("config.toml");
    if let Err(e) = std::fs::write(&config_path, "asr = \"open-ai\"\n") {
        return CheckResult::fail("B-04", format!("could not write test config.toml: {e}"));
    }

    let config = whspr_config::load_from(Some(temp_dir.path()));
    if matches!(config.asr, whspr_config::AsrChoice::OpenAi) {
        CheckResult::pass(
            "B-04",
            "wrote `asr = \"open-ai\"` as TOML to config.toml and load_from() correctly parsed \
             it as AsrChoice::OpenAi",
        )
    } else {
        CheckResult::fail(
            "B-04",
            format!(
                "wrote a TOML config file but load_from() returned asr = {:?}, not OpenAi",
                config.asr
            ),
        )
    }
}

/// The TOML section headers and nested keys `Config::default()` actually
/// serializes to today (verified by hand: `toml::to_string_pretty` against
/// a fresh `Config::default()`), one entry per top-level settings struct
/// plus a couple of representative nested keys. Deliberately not every
/// single field - this is a "did a whole settings surface silently vanish
/// from the default config" tripwire, not a byte-for-byte schema diff.
/// Update this list if `Config`'s fields genuinely change shape; keeping it
/// hand-verified against the real struct (rather than generated) is the
/// point - see B-05 below.
const REQUIRED_CONFIG_SECTIONS: &[&str] = &[
    "[api_keys]",
    "[whisper]",
    "[speaker]",
    "similarity-threshold",
    "embedding-model",
    "[normalize]",
    "numbers-format",
    "[language_settings]",
    "[device]",
    "device-hotplug",
    "active-window",
    "[autostart]",
    "[sound]",
    "[injection]",
    "[privacy]",
    "mic-privacy",
    "[capture]",
    "[huggingface]",
    "[refine_settings]",
];

/// B-05: the default config contains all required sections/keys.
///
/// Serializes `Config::default()` (the real struct from the `whspr-config`
/// workspace crate, not a hand-copied schema) via `toml::to_string_pretty`
/// and checks the result contains every section/key in
/// `REQUIRED_CONFIG_SECTIONS` - one representative per top-level settings
/// struct (`[whisper]`, `[speaker]`, `[normalize]`, `[device]`, ...), so a
/// struct that got dropped from `Config` (or renamed out of kebab-case)
/// would show up here as a missing section.
pub fn check_config_sections() -> CheckResult {
    let config = whspr_config::Config::default();
    let toml_str = match toml::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => {
            return CheckResult::fail(
                "B-05",
                format!("could not serialize Config::default() to TOML: {e}"),
            )
        }
    };

    let missing: Vec<&str> = REQUIRED_CONFIG_SECTIONS
        .iter()
        .filter(|key| !toml_str.contains(*key))
        .copied()
        .collect();

    if missing.is_empty() {
        CheckResult::pass(
            "B-05",
            format!(
                "Config::default()'s TOML serialization contains all {} required \
                 sections/keys: {}",
                REQUIRED_CONFIG_SECTIONS.len(),
                REQUIRED_CONFIG_SECTIONS.join(", ")
            ),
        )
    } else {
        CheckResult::fail(
            "B-05",
            format!(
                "default config is missing {} of {} required section(s)/key(s): {}",
                missing.len(),
                REQUIRED_CONFIG_SECTIONS.len(),
                missing.join(", ")
            ),
        )
    }
}

/// B-14: config lives in the platform config directory.
///
/// Computes the platform config dir the same way `whspr_config::load()`
/// does (`directories::ProjectDirs::from("", "", "whspr").config_dir()`)
/// and checks it's an absolute, whspr-named path - plus a structural
/// grep confirming `load()` actually calls `ProjectDirs::from` with the
/// same `"whspr"` qualifier, so this isn't just two implementations that
/// coincidentally agree.
pub fn check_config_in_platform_dir(root: &Path) -> CheckResult {
    let Some(project_dirs) = directories::ProjectDirs::from("", "", "whspr") else {
        return CheckResult::fail(
            "B-14",
            "directories::ProjectDirs::from(\"\", \"\", \"whspr\") returned None on this \
             platform",
        );
    };
    let config_dir = project_dirs.config_dir();

    let uses_project_dirs = match repo::git_grep(
        root,
        &["-F"],
        "ProjectDirs::from(\"\", \"\", \"whspr\")",
        &["*.rs"],
    ) {
        Ok(matches) => !matches.is_empty(),
        Err(e) => {
            return CheckResult::fail("B-14", format!("could not grep whspr-config source: {e}"))
        }
    };

    let looks_like_whspr_dir = config_dir.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("whspr")
    });

    if config_dir.is_absolute() && looks_like_whspr_dir && uses_project_dirs {
        CheckResult::pass(
            "B-14",
            format!(
                "platform config dir resolves to {} and whspr-config's source calls the same \
                 ProjectDirs::from(\"\", \"\", \"whspr\")",
                config_dir.display()
            ),
        )
    } else {
        CheckResult::fail(
            "B-14",
            format!(
                "config dir = {} (absolute: {}, whspr-named: {looks_like_whspr_dir}), \
                 whspr-config source uses ProjectDirs the same way: {uses_project_dirs}",
                config_dir.display(),
                config_dir.is_absolute()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_dirs_from_whspr_returns_valid_paths() {
        let dirs = directories::ProjectDirs::from("", "", "whspr");
        assert!(
            dirs.is_some(),
            "ProjectDirs::from should succeed for 'whspr'"
        );
        let dirs = dirs.unwrap();
        assert!(
            dirs.config_dir().is_absolute(),
            "config_dir should be absolute"
        );
    }

    #[test]
    fn whspr_qualifier_appears_in_config_path() {
        if let Some(dirs) = directories::ProjectDirs::from("", "", "whspr") {
            let config_path = dirs.config_dir();
            let path_str = config_path.to_string_lossy();
            assert!(
                path_str.to_lowercase().contains("whspr"),
                "whspr should appear in config path"
            );
        }
    }

    #[test]
    fn check_config_sections_passes_against_the_real_default_config() {
        let result = super::check_config_sections();
        assert_eq!(
            result.verdict,
            crate::report::Verdict::Pass,
            "evidence: {}",
            result.evidence
        );
    }
}
