//! The `whspr uninstall` subcommand (AH-08): removes the autostart entry
//! (if any) and the app's config/data directories. Split out of `main.rs`
//! to keep that file under this project's 600-line-per-file guideline,
//! same reasoning as `diarize_cmd.rs`/`stats_cmd.rs`.

use std::path::{Path, PathBuf};

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
pub async fn run(data_dir: Option<PathBuf>, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!(
            "This would remove whspr's autostart entry and its config/data \
             directories. Re-run with --yes to actually do it."
        );
        return Ok(());
    }

    // `data_dir` (hidden, test-only --data-dir) being set means this is
    // the e2e suite exercising the removal logic in isolation, not a real
    // uninstall - skip the actual OS-level autostart removal so tests
    // never touch the real current user's LaunchAgents/autostart entry.
    if data_dir.is_none() {
        match whspr_config::remove_autostart() {
            Ok(()) => println!("Removed autostart entry (if one existed)."),
            Err(e) => println!("Warning: could not remove autostart entry: {e}"),
        }
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
    use super::*;

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
