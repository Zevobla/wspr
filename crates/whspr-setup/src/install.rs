//! The real install: what pressing **Install** actually does on Windows.
//!
//! The install is modelled as an ordered list of [`Job`]s ([`plan`]), each run
//! one at a time off the UI thread (see `crate::run_job`) so the progress bar
//! advances through the *real* steps and any failure routes to the Failure
//! screen. On Windows [`perform`] does the genuine work:
//!
//! 1. create `%LOCALAPPDATA%\whspr`,
//! 2. write the embedded [`crate::payload`] there (the app exe + any DLLs),
//! 3. optionally create a per-user Start-menu shortcut,
//! 4. optionally create a Desktop shortcut.
//!
//! Everything platform-specific -- `%LOCALAPPDATA%`, shortcut creation -- is
//! behind `#[cfg(target_os = "windows")]`. Off Windows the installer is never
//! shipped, so [`perform`] is a no-op that lets the shared UI still build and
//! run (a simulated walk) for the macOS gate.

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};

/// One real install action. The fixed ordering of the variants is also their
/// execution order (see [`plan`]) and the order [`Job::start_progress`]
/// increases along, so the bar never moves backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// Create the install directory `%LOCALAPPDATA%\whspr`.
    CreateDir,
    /// Write the embedded payload (app exe + DLLs) into the install dir.
    WritePayload,
    /// Create the per-user Start-menu `whspr.lnk`.
    StartMenuShortcut,
    /// Create the Desktop `whspr.lnk`.
    DesktopShortcut,
    /// Terminal marker so the bar reaches 100% under "Finishing up".
    Finish,
}

impl Job {
    /// The progress-bar value (0..=100) shown *while this job runs*. Chosen so
    /// each job sits in the matching `crate::state::Step` label band -- copy
    /// (0..24), shortcuts (25..49), finishing (75..100) -- and so the value
    /// strictly increases in execution order even when the optional jobs are
    /// absent, keeping the bar monotonic.
    pub fn start_progress(self) -> u8 {
        match self {
            Job::CreateDir => 5,
            Job::WritePayload => 18,
            Job::StartMenuShortcut => 30,
            Job::DesktopShortcut => 42,
            Job::Finish => 100,
        }
    }
}

/// Builds the ordered job list for the chosen options. `CreateDir` and
/// `Finish` always run; the payload copy runs only when something was embedded
/// (skipped in the empty-payload macOS/CI build); shortcuts follow their
/// toggles.
pub fn plan(start_menu: bool, desktop: bool) -> Vec<Job> {
    let mut jobs = vec![Job::CreateDir];
    if !crate::payload::PAYLOAD.is_empty() {
        jobs.push(Job::WritePayload);
    }
    if start_menu {
        jobs.push(Job::StartMenuShortcut);
    }
    if desktop {
        jobs.push(Job::DesktopShortcut);
    }
    jobs.push(Job::Finish);
    jobs
}

/// Runs one install `job`, returning a human-readable error on failure.
/// Windows does the real work; every other target is a no-op (the installer is
/// Windows-only) so the shared UI still builds and the macOS gate can run a
/// simulated walk.
pub fn perform(job: Job) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        perform_windows(job)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = job;
        Ok(())
    }
}

// -- Windows implementation --------------------------------------------------

/// The install directory's name under `%LOCALAPPDATA%`.
#[cfg(target_os = "windows")]
const INSTALL_SUBDIR: &str = "whspr";

/// The launched executable's name, used when the payload embeds no `.exe`
/// (e.g. an empty-payload dev build) so shortcut targets still resolve to a
/// sensible path.
#[cfg(target_os = "windows")]
const DEFAULT_APP_EXE: &str = "whspr-app.exe";

#[cfg(target_os = "windows")]
fn perform_windows(job: Job) -> Result<(), String> {
    match job {
        Job::CreateDir => create_install_dir(),
        Job::WritePayload => write_payload(),
        Job::StartMenuShortcut => create_shortcut(ShortcutKind::StartMenu),
        Job::DesktopShortcut => create_shortcut(ShortcutKind::Desktop),
        Job::Finish => Ok(()),
    }
}

/// `%LOCALAPPDATA%\whspr` -- the per-user install directory (no admin needed).
#[cfg(target_os = "windows")]
fn install_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "the LOCALAPPDATA environment variable is not set".to_string())?;
    Ok(Path::new(&base).join(INSTALL_SUBDIR))
}

/// The installed executable's full path (`<install dir>\<app exe>`).
#[cfg(target_os = "windows")]
fn installed_exe() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(app_exe_name()))
}

/// The payload's `.exe` entry name (there is exactly one), or
/// [`DEFAULT_APP_EXE`] when the payload is empty.
#[cfg(target_os = "windows")]
fn app_exe_name() -> &'static str {
    crate::payload::PAYLOAD
        .iter()
        .map(|&(name, _)| name)
        .find(|name| name.to_ascii_lowercase().ends_with(".exe"))
        .unwrap_or(DEFAULT_APP_EXE)
}

#[cfg(target_os = "windows")]
fn create_install_dir() -> Result<(), String> {
    let dir = install_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))
}

#[cfg(target_os = "windows")]
fn write_payload() -> Result<(), String> {
    let dir = install_dir()?;
    for &(name, bytes) in crate::payload::PAYLOAD {
        let dest = dir.join(name);
        std::fs::write(&dest, bytes).map_err(|e| format!("could not write {}: {e}", dest.display()))?;
    }
    Ok(())
}

/// Which per-user special folder a shortcut lands in.
#[cfg(target_os = "windows")]
enum ShortcutKind {
    StartMenu,
    Desktop,
}

#[cfg(target_os = "windows")]
impl ShortcutKind {
    /// The `System.Environment.SpecialFolder` name PowerShell resolves. Using
    /// the enum name (rather than a hard-coded path) means Desktop redirection
    /// (e.g. OneDrive) and the real per-user Programs folder are honored.
    fn special_folder(&self) -> &'static str {
        match self {
            ShortcutKind::StartMenu => "Programs",
            ShortcutKind::Desktop => "Desktop",
        }
    }
}

/// The shortcut-creation script. Every value comes in through the environment
/// (never string-interpolated), so paths with spaces (`C:\Users\Jane Doe\...`)
/// and the "Start Menu" folder need no quoting and can't break the command.
#[cfg(target_os = "windows")]
const SHORTCUT_SCRIPT: &str = "\
$ErrorActionPreference = 'Stop'; \
$dir = [Environment]::GetFolderPath($env:WHSPR_FOLDER); \
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }; \
$lnk = Join-Path $dir $env:WHSPR_LNKNAME; \
$ws = New-Object -ComObject WScript.Shell; \
$sc = $ws.CreateShortcut($lnk); \
$sc.TargetPath = $env:WHSPR_TARGET; \
$sc.WorkingDirectory = $env:WHSPR_WORKDIR; \
$sc.Save()";

/// Creates a `whspr.lnk` in the chosen folder pointing at the installed exe,
/// via `WScript.Shell.CreateShortcut` under PowerShell (no COM plumbing, no
/// extra crate). Robust to spaces; any non-zero exit becomes an `Err`.
#[cfg(target_os = "windows")]
fn create_shortcut(kind: ShortcutKind) -> Result<(), String> {
    let target = installed_exe()?;
    let workdir = install_dir()?;
    let folder = kind.special_folder();

    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SHORTCUT_SCRIPT,
        ])
        .env("WHSPR_FOLDER", folder)
        .env("WHSPR_LNKNAME", "whspr.lnk")
        .env("WHSPR_TARGET", &target)
        .env("WHSPR_WORKDIR", &workdir)
        .status()
        .map_err(|e| format!("could not run PowerShell to create the {folder} shortcut: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "PowerShell exited unsuccessfully ({status}) creating the {folder} shortcut"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_always_creates_the_dir_and_finishes() {
        let jobs = plan(false, false);
        assert_eq!(jobs.first(), Some(&Job::CreateDir));
        assert_eq!(jobs.last(), Some(&Job::Finish));
    }

    #[test]
    fn plan_includes_only_the_enabled_shortcut_jobs() {
        let both = plan(true, true);
        assert!(both.contains(&Job::StartMenuShortcut));
        assert!(both.contains(&Job::DesktopShortcut));

        let none = plan(false, false);
        assert!(!none.contains(&Job::StartMenuShortcut));
        assert!(!none.contains(&Job::DesktopShortcut));
    }

    #[test]
    fn start_progress_increases_in_execution_order_and_ends_at_100() {
        let order = [
            Job::CreateDir,
            Job::WritePayload,
            Job::StartMenuShortcut,
            Job::DesktopShortcut,
            Job::Finish,
        ];
        for pair in order.windows(2) {
            assert!(
                pair[0].start_progress() < pair[1].start_progress(),
                "{:?} must sit before {:?} on the bar",
                pair[0],
                pair[1]
            );
        }
        assert_eq!(Job::Finish.start_progress(), 100);
    }
}
