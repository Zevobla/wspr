//! The `whspr uninstall` subcommand (AH-08): removes the autostart entry
//! (if any), whspr's keychain entries, and the app's config/data
//! directories. Split out of `main.rs`
//! to keep that file under this project's 600-line-per-file guideline,
//! same reasoning as `diarize_cmd.rs`/`stats_cmd.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use whspr_config::{Config, Keystore, SecretName};

/// Cloud backends the desktop app offers an API-key field for; their keys
/// may sit in the keychain even when `[api_keys]` no longer names them.
const KNOWN_KEY_BACKENDS: &[&str] = &["openai", "anthropic", "deepgram"];

/// Removes a directory tree, tolerating "it doesn't exist" as a normal
/// outcome rather than an error - same reasoning as
/// `whspr_config::autostart`'s `remove_if_exists`. Returns whether
/// anything was actually removed, so the caller can report it honestly.
fn remove_dir_if_exists(path: &Path) -> anyhow::Result<bool> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Every keystore entry whspr may have created: an API key for each known
/// cloud backend and each backend named in `[api_keys]`, the HuggingFace
/// token, and the history-encryption key.
fn whspr_secret_names(config: &Config) -> Vec<String> {
    let backends: BTreeSet<&str> = KNOWN_KEY_BACKENDS
        .iter()
        .copied()
        .chain(config.api_keys.keys().map(String::as_str))
        .collect();
    let mut names: Vec<String> = backends.into_iter().map(SecretName::api_key).collect();
    names.push(SecretName::HF_TOKEN.to_string());
    names.push(SecretName::HISTORY_KEY.to_string());
    names
}

/// Deletes whspr's keystore entries, reporting (not aborting on) a failure.
/// Deleting an entry that was never there is not a failure.
fn remove_secrets(config: &Config, keystore: &dyn Keystore) {
    let mut failures = 0;
    for name in whspr_secret_names(config) {
        if let Err(e) = keystore.delete(&name) {
            failures += 1;
            println!("Warning: could not remove keychain entry {name}: {e}");
        }
    }
    if failures == 0 {
        println!("Removed whspr's keychain entries (if any existed).");
    }
}

/// Resolves the config and data directories to remove. `override_dir`
/// (the hidden `--data-dir` test flag) redirects *both* to the same
/// single directory, mirroring how the rest of the CLI's `--data-dir`
/// override works - a test proving the removal logic doesn't need two
/// separate temp dirs to stand in for the real, platform-specific ones.
fn resolve_dirs(override_dir: Option<&Path>) -> anyhow::Result<(PathBuf, PathBuf)> {
    if let Some(dir) = override_dir {
        return Ok((dir.to_path_buf(), dir.to_path_buf()));
    }
    let dirs = directories::ProjectDirs::from("", "", "whspr")
        .ok_or_else(|| anyhow::anyhow!("cannot determine platform config/data dirs"))?;
    Ok((
        dirs.config_dir().to_path_buf(),
        dirs.data_dir().to_path_buf(),
    ))
}

/// Runs the `uninstall` subcommand: removes the autostart entry and the
/// app's config/data directories, printing exactly what it did (or would
/// do). Honest, best-effort cleanup - a failure on one step (e.g.
/// autostart isn't implemented on this platform) is reported and skipped
/// rather than aborting the rest, and nothing here ever panics.
///
/// Without `--yes` this only prints what it *would* remove: destructive,
/// irreversible deletion needs an explicit opt-in rather than running by
/// default the moment someone types the subcommand name.
pub async fn run(
    config: &Config,
    keystore: &dyn Keystore,
    data_dir: Option<PathBuf>,
    yes: bool,
) -> anyhow::Result<()> {
    if !yes {
        println!(
            "This would remove whspr's autostart entry, its keychain entries (saved \
             API keys, HuggingFace token, history key) and its config/data \
             directories. Re-run with --yes to actually do it."
        );
        return Ok(());
    }

    // `data_dir` (hidden, test-only --data-dir) being set means this is
    // the e2e suite exercising the removal logic in isolation, not a real
    // uninstall - skip the actual OS-level autostart and keychain removal
    // so tests never touch the real current user's LaunchAgents/autostart
    // entry or login keychain.
    if data_dir.is_none() {
        match whspr_config::remove_autostart() {
            Ok(()) => println!("Removed autostart entry (if one existed)."),
            Err(e) => println!("Warning: could not remove autostart entry: {e}"),
        }
        remove_secrets(config, keystore);
    }

    let (config_dir, data_dir_path) = resolve_dirs(data_dir.as_deref())?;

    match remove_dir_if_exists(&config_dir) {
        Ok(true) => println!("Removed config directory: {}", config_dir.display()),
        Ok(false) => println!(
            "No config directory to remove ({} does not exist)",
            config_dir.display()
        ),
        Err(e) => println!(
            "Warning: could not remove config directory {}: {e}",
            config_dir.display()
        ),
    }

    // On macOS these two resolve to the same path; don't report (or try
    // to remove) it twice.
    if data_dir_path != config_dir {
        match remove_dir_if_exists(&data_dir_path) {
            Ok(true) => println!("Removed data directory: {}", data_dir_path.display()),
            Ok(false) => println!(
                "No data directory to remove ({} does not exist)",
                data_dir_path.display()
            ),
            Err(e) => println!(
                "Warning: could not remove data directory {}: {e}",
                data_dir_path.display()
            ),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use whspr_config::MemoryKeystore;

    use super::*;

    #[test]
    fn secret_names_cover_known_and_configured_backends_once() {
        let mut config = Config::default();
        config.api_keys.insert("openai".into(), "k".into());
        config.api_keys.insert("custom".into(), "k".into());
        let names = whspr_secret_names(&config);
        assert_eq!(
            names,
            vec![
                SecretName::api_key("anthropic"),
                SecretName::api_key("custom"),
                SecretName::api_key("deepgram"),
                SecretName::api_key("openai"),
                SecretName::HF_TOKEN.to_string(),
                SecretName::HISTORY_KEY.to_string(),
            ]
        );
    }

    #[test]
    fn remove_secrets_clears_every_whspr_entry() {
        let keystore = MemoryKeystore::default();
        for name in whspr_secret_names(&Config::default()) {
            keystore.set(&name, "value").unwrap();
        }
        keystore.set("not-whspr", "kept").unwrap();
        remove_secrets(&Config::default(), &keystore);
        for name in whspr_secret_names(&Config::default()) {
            assert_eq!(keystore.get(&name).unwrap(), None, "{name} survived");
        }
        assert_eq!(keystore.get("not-whspr").unwrap().as_deref(), Some("kept"));
    }

    #[test]
    fn remove_dir_if_exists_removes_a_present_directory() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let target = dir.path().join("to-remove");
        std::fs::create_dir_all(target.join("nested")).expect("failed to create nested dir");
        std::fs::write(target.join("nested").join("file.txt"), b"hi").expect("failed to write");

        let removed = remove_dir_if_exists(&target).expect("removal should succeed");
        assert!(removed);
        assert!(!target.exists());
    }

    #[test]
    fn remove_dir_if_exists_tolerates_a_missing_directory() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let target = dir.path().join("never-existed");

        let removed = remove_dir_if_exists(&target).expect("a missing dir is not an error");
        assert!(!removed);
    }

    #[test]
    fn resolve_dirs_override_points_both_at_the_same_directory() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let (config_dir, data_dir) =
            resolve_dirs(Some(dir.path())).expect("an explicit override should always resolve");
        assert_eq!(config_dir, dir.path());
        assert_eq!(data_dir, dir.path());
    }
}
