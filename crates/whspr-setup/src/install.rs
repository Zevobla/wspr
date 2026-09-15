//! The real install: what pressing **Install** actually does on Windows.
//!
//! The install is modelled as an ordered list of [`Job`]s ([`plan`]), each run
//! one at a time off the UI thread (see `crate::run_job`) so the progress bar
//! advances through the *real* steps and any failure routes to the Failure
//! screen. On Windows [`perform`] does the genuine work: create
//! `%LOCALAPPDATA%\whspr` and write the embedded [`crate::payload`] there (the
//! app exe + any DLLs).
//!
//! Everything platform-specific -- `%LOCALAPPDATA%`, the file writes -- is
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
    /// Terminal marker so the bar reaches 100% under "Finishing up".
    Finish,
}

impl Job {
    /// The progress-bar value (0..=100) shown *while this job runs*. Chosen so
    /// each job sits in the matching `crate::state::Step` label band -- copy
    /// (0..24), finishing (75..100) -- and so the value strictly increases in
    /// execution order, keeping the bar monotonic.
    pub fn start_progress(self) -> u8 {
        match self {
            Job::CreateDir => 5,
            Job::WritePayload => 18,
            Job::Finish => 100,
        }
    }
}

/// Builds the ordered job list. `CreateDir` and `Finish` always run; the
/// payload copy runs only when something was embedded (skipped in the
/// empty-payload macOS/CI build).
pub fn plan() -> Vec<Job> {
    let mut jobs = vec![Job::CreateDir];
    if !crate::payload::PAYLOAD.is_empty() {
        jobs.push(Job::WritePayload);
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

#[cfg(target_os = "windows")]
fn perform_windows(job: Job) -> Result<(), String> {
    match job {
        Job::CreateDir => create_install_dir(),
        Job::WritePayload => write_payload(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_always_creates_the_dir_and_finishes() {
        let jobs = plan();
        assert_eq!(jobs.first(), Some(&Job::CreateDir));
        assert_eq!(jobs.last(), Some(&Job::Finish));
    }

    #[test]
    fn start_progress_increases_in_execution_order_and_ends_at_100() {
        let order = [Job::CreateDir, Job::WritePayload, Job::Finish];
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
