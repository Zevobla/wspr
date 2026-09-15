//! Best-effort OS "launch at login" integration (B-10): writes or removes
//! a platform autostart entry pointing at whspr's own executable.
//!
//! Dependency-free by design (no `plist`/freedesktop crate -- see
//! CLAUDE.md's "don't add a new workspace dependency" rule): the plist/
//! `.desktop` file contents are small enough to hand-write as a format
//! string. Every OS write here is guarded to never panic -- a permission
//! error, or a platform whspr doesn't support autostart on yet, becomes an
//! `Err` the caller can surface (e.g. via the Hub's `last_error`), rather
//! than a silent no-op that leaves the user thinking the toggle worked.
//!
//! Two platforms are implemented:
//! - **macOS**: a LaunchAgent plist at `~/Library/LaunchAgents/<id>.plist`
//!   with `RunAtLoad`.
//! - **Linux**: an XDG autostart entry at `~/.config/autostart/whspr.desktop`
//!   (`directories::BaseDirs::config_dir()` resolves to `~/.config` there).
//!
//! **Windows is not implemented** -- it needs a `HKCU\...\Run` registry
//! key, which means either a `winreg`-style crate (a new workspace
//! dependency) or raw `windows-sys` FFI, neither of which is justified for
//! one setting. `install_autostart`/`remove_autostart` return a clear
//! error there instead of pretending to succeed.
//!
//! The path-building and file-writing helpers below are deliberately plain
//! functions (no `#[cfg(target_os = ...)]`), so they're typechecked and
//! unit-testable on every platform regardless of which one is actually
//! running -- only the public `install_autostart`/`remove_autostart`
//! branch on `cfg!(target_os = ...)` at runtime to pick which one to call.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use whspr_core::{Result, WhsprError};

/// Whether whspr should launch automatically at login. `enabled` is just
/// the persisted *intent* -- toggling it in the GUI also writes/removes
/// the actual OS-level entry via `install_autostart`/`remove_autostart`
/// below, but this field alone doesn't prove that entry exists (e.g. it
/// could have been deleted out from under whspr by the user or the OS).
///
/// Defined here rather than alongside `Config`'s other settings structs in
/// `lib.rs` (like `SpeakerSettings`/`NormalizeSettings`) so that crate's
/// file stays under this project's 600-line-per-file guideline (AA-06) --
/// same reasoning `speaker.rs` already follows for `SpeakerDb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AutostartSettings {
    pub enabled: bool,
}

/// Reverse-DNS-style identifier used for the macOS LaunchAgent's `Label`
/// and plist filename -- stable across installs so a repeat
/// `install_autostart` overwrites rather than duplicates the entry.
const AUTOSTART_ID: &str = "com.whspr.app";

/// The `HKCU\...\Run` value NAME whspr registers itself under on Windows.
/// Stable across installs (like `AUTOSTART_ID` for the LaunchAgent), so a
/// repeat `install_autostart` overwrites the same value rather than piling
/// up duplicates. Only read by the Windows arms and the unit test below --
/// `allow(dead_code)` covers the macOS/Linux non-test build, where it's
/// intentionally unreferenced but kept compiled so the test can assert it.
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
const RUN_VALUE_NAME: &str = "whspr";

/// The `HKCU\...\Run` value DATA for `exe`: the executable path wrapped in
/// double quotes so a path containing spaces (e.g.
/// `C:\Program Files\whspr\whspr-app.exe`) stays a single `argv[0]` when
/// Windows launches it at login, rather than being split on the space.
///
/// Parameterized on `exe` (rather than calling `current_exe()` itself) so
/// it's a pure function unit-testable on every OS -- exactly like
/// `launchagent_plist_contents`. The Windows install arm passes the real
/// `std::env::current_exe()`.
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
fn run_value_data(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The per-user registry sub-key (under `HKEY_CURRENT_USER`) Windows reads
/// at login to launch autostart programs. Windows-only, so it's `#[cfg]`'d
/// out entirely on other platforms rather than left as dead code.
#[cfg(target_os = "windows")]
const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Installs a "launch at login" entry pointing at `binary_path` (the
/// running app's own executable -- callers should pass
/// `std::env::current_exe()`).
pub fn install_autostart(binary_path: &Path) -> Result<()> {
    // Windows uses the registry (via the Windows-only `winreg` crate), so it
    // delegates to a `#[cfg]`'d helper -- a runtime `cfg!` guard (rather than
    // a `#[cfg]` on this line) keeps the macOS/Linux code below compiled and
    // reference-clean on every target, mirroring `whspr-hf`'s cfg'd fn pair.
    if cfg!(target_os = "windows") {
        return install_autostart_windows();
    }

    let base = home_dirs()?;

    if cfg!(target_os = "macos") {
        write_launchagent_plist(&plist_path(&base), binary_path).map_err(autostart_err)
    } else if cfg!(target_os = "linux") {
        write_xdg_desktop_entry(&desktop_entry_path(&base), binary_path).map_err(autostart_err)
    } else {
        let _ = binary_path;
        Err(unsupported_platform_err())
    }
}

/// Removes whatever autostart entry `install_autostart` would have
/// written, tolerating "there wasn't one" (not-found is not an error --
/// mirrors `SpeakerDb::load`'s "missing file is fine" reasoning).
pub fn remove_autostart() -> Result<()> {
    let base = home_dirs()?;

    if cfg!(target_os = "macos") {
        remove_if_exists(&plist_path(&base)).map_err(autostart_err)
    } else if cfg!(target_os = "linux") {
        remove_if_exists(&desktop_entry_path(&base)).map_err(autostart_err)
    } else {
        Err(unsupported_platform_err())
    }
}

fn home_dirs() -> Result<directories::BaseDirs> {
    directories::BaseDirs::new()
        .ok_or_else(|| WhsprError::Config("could not determine home directory".into()))
}

fn unsupported_platform_err() -> WhsprError {
    WhsprError::Config(
        "launch-at-login isn't implemented on this platform yet (only macOS and Linux are supported)"
            .into(),
    )
}

fn autostart_err(e: io::Error) -> WhsprError {
    WhsprError::Config(format!("failed to update autostart entry: {e}"))
}

/// Where the macOS LaunchAgent plist lives for the current user.
fn plist_path(base: &directories::BaseDirs) -> PathBuf {
    base.home_dir()
        .join("Library/LaunchAgents")
        .join(format!("{AUTOSTART_ID}.plist"))
}

/// Where the Linux XDG autostart `.desktop` entry lives for the current
/// user.
fn desktop_entry_path(base: &directories::BaseDirs) -> PathBuf {
    base.config_dir().join("autostart").join("whspr.desktop")
}

/// Writes a LaunchAgent plist to `path` (creating any missing parent
/// directory), pointing `ProgramArguments` at `binary_path`. Takes the
/// full file path rather than resolving `~/Library/LaunchAgents` itself,
/// so it's unit-testable against a tempdir.
fn write_launchagent_plist(path: &Path, binary_path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, launchagent_plist_contents(binary_path))
}

fn launchagent_plist_contents(binary_path: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AUTOSTART_ID}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        binary_path.display()
    )
}

/// Writes an XDG autostart `.desktop` entry to `path` (creating any
/// missing parent directory), pointing `Exec` at `binary_path`. Same
/// full-path-in, tempdir-testable shape as `write_launchagent_plist`.
fn write_xdg_desktop_entry(path: &Path, binary_path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, xdg_desktop_entry_contents(binary_path))
}

fn xdg_desktop_entry_contents(binary_path: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=whspr\nExec={}\nX-GNOME-Autostart-enabled=true\n",
        binary_path.display()
    )
}

/// Removes `path`, tolerating "it doesn't exist" as success rather than an
/// error -- there's nothing to remove on a fresh install or a repeat
/// `remove_autostart` call.
fn remove_if_exists(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Writes the `whspr` autostart value into the current user's `Run` key,
/// creating/opening it with write access and overwriting any existing value
/// of the same name (idempotent re-install). The exe path comes from
/// `current_exe()` (the pure `run_value_data` only quotes it). winreg
/// surfaces registry failures as `io::Error`, so they funnel through
/// `autostart_err` like every other write in this module.
#[cfg(target_os = "windows")]
fn install_autostart_windows() -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WRITE};
    use winreg::RegKey;

    let exe = std::env::current_exe().map_err(autostart_err)?;
    let (run_key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(RUN_KEY_PATH, KEY_SET_VALUE | KEY_WRITE)
        .map_err(autostart_err)?;
    run_key
        .set_value(RUN_VALUE_NAME, &run_value_data(&exe))
        .map_err(autostart_err)
}

/// Off Windows this is never reached -- the `cfg!(target_os = "windows")`
/// guard in `install_autostart` is false -- but the call site still needs a
/// symbol to reference on every target, so a stub stands in (same cfg'd-pair
/// shape as `whspr-hf`'s `recommended_working_set`).
#[cfg(not(target_os = "windows"))]
fn install_autostart_windows() -> Result<()> {
    Err(unsupported_platform_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_launchagent_plist_contains_the_label_and_binary_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("com.whspr.app.plist");
        let binary = PathBuf::from("/Applications/whspr.app/Contents/MacOS/whspr-app");

        write_launchagent_plist(&path, &binary).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains(AUTOSTART_ID));
        assert!(contents.contains(binary.to_str().unwrap()));
        assert!(contents.contains("RunAtLoad"));
    }

    #[test]
    fn write_launchagent_plist_creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("nested")
            .join("LaunchAgents")
            .join("x.plist");

        write_launchagent_plist(&path, Path::new("/usr/local/bin/whspr-app")).unwrap();

        assert!(path.is_file());
    }

    #[test]
    fn write_xdg_desktop_entry_contains_exec_and_binary_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("whspr.desktop");
        let binary = PathBuf::from("/usr/bin/whspr-app");

        write_xdg_desktop_entry(&path, &binary).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[Desktop Entry]"));
        assert!(contents.contains(&format!("Exec={}", binary.display())));
    }

    #[test]
    fn remove_if_exists_tolerates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nonexistent");
        assert!(remove_if_exists(&missing).is_ok());
    }

    #[test]
    fn remove_if_exists_removes_a_present_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("present");
        std::fs::write(&path, b"x").unwrap();

        remove_if_exists(&path).unwrap();

        assert!(!path.exists());
    }

    /// Sanity check the two path-builder fns are actually distinct and
    /// rooted at the resolved home directory, not hardcoded strings that
    /// happened to coincide.
    #[test]
    fn plist_path_and_desktop_entry_path_are_distinct_and_under_home() {
        let base = directories::BaseDirs::new().expect("should resolve a home dir in test env");
        assert_ne!(plist_path(&base), desktop_entry_path(&base));
        assert!(plist_path(&base).starts_with(base.home_dir()));
    }

    /// The Windows Run value must quote the exe path (so a `C:\Program
    /// Files\...` path with a space stays one `argv[0]`) and register under
    /// the stable `whspr` name. Pure -- runs on every OS, including this
    /// macOS gate.
    #[test]
    fn run_value_data_quotes_a_spaced_exe_path() {
        let exe = Path::new(r"C:\Program Files\whspr\whspr-app.exe");
        let data = run_value_data(exe);

        assert_eq!(data, r#""C:\Program Files\whspr\whspr-app.exe""#);
        assert!(data.starts_with('"') && data.ends_with('"'));
        assert_eq!(RUN_VALUE_NAME, "whspr");
    }

    // `Config`/`load_from` live in the crate root, not this module, but
    // these three exercise `AutostartSettings` through them the same way
    // `lib.rs`'s test module does for `SpeakerSettings`/`NormalizeSettings`.
    use crate::{load_from, Config};

    #[test]
    fn autostart_settings_defaults_to_disabled() {
        assert_eq!(
            Config::default().autostart,
            AutostartSettings { enabled: false }
        );
    }

    #[test]
    fn autostart_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.autostart.enabled = true;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.autostart, cfg.autostart);
    }

    #[test]
    fn load_from_toml_file_sets_autostart_enabled() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[autostart]").expect("failed to write autostart header");
        writeln!(file, "enabled = true").expect("failed to write enabled");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(cfg.autostart.enabled);
    }
}
