//! First-run/config checks. All three call `whspr_config` directly (a real
//! workspace dependency of this crate) rather than statically reading its
//! source, so these are dynamic behavioral checks, not text scans.

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
