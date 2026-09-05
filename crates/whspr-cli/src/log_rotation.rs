//! Size-based rotation for whspr's append-only log files (O-19).
//!
//! There's no `tracing`-to-file sink installed anywhere in this workspace
//! today (`tracing::error!` calls, e.g. `whspr-audio`'s cpal stream error
//! callbacks, currently go nowhere -- no subscriber is registered), so the
//! only file that actually grows forever is `save_to_history`'s
//! `history.jsonl` journal. This module rotates *that* file; it isn't
//! tied to `history.jsonl` specifically, so it's ready to rotate a real
//! log file too if/when one exists.
//!
//! Dependency-free by design: a `std::fs::metadata` size check plus a
//! single `std::fs::rename` to roll the oversized file to `<path>.1`
//! (overwriting whatever `.1` was already there), so the caller's next
//! write starts a fresh, empty file. No third-party log-rotation crate
//! needed for a rule this simple.

use std::io;
use std::path::{Path, PathBuf};

/// Default size threshold for `history.jsonl`: comfortably holds tens of
/// thousands of dictation turns as compact JSON lines before rolling.
pub const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB

/// If `path` exists and is at least `max_bytes` large, renames it to
/// `<path>.1` (silently replacing any existing `.1`), so the caller's next
/// write starts a fresh, empty file. A missing `path` is not an error --
/// nothing to rotate yet on a fresh install, mirroring `SpeakerDb::load`'s
/// "missing file is fine" reasoning.
pub fn rotate_if_too_large(path: &Path, max_bytes: u64) -> io::Result<()> {
    let size = match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if size >= max_bytes {
        std::fs::rename(path, rolled_path(path))?;
    }

    Ok(())
}

/// The `<path>.1` rotation target. Pure so the naming rule is testable on
/// its own, independent of any real file I/O.
fn rolled_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".1");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolled_path_appends_dot_one() {
        assert_eq!(
            rolled_path(Path::new("/tmp/history.jsonl")),
            Path::new("/tmp/history.jsonl.1")
        );
    }

    #[test]
    fn rotate_leaves_a_small_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path, b"small").unwrap();

        rotate_if_too_large(&path, 1024).unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "small");
        assert!(!rolled_path(&path).exists());
    }

    #[test]
    fn rotate_rolls_an_oversized_file_to_dot_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path, b"0123456789").unwrap(); // 10 bytes

        rotate_if_too_large(&path, 10).unwrap();

        assert!(
            !path.exists(),
            "oversized file should have been rolled away"
        );
        assert_eq!(
            std::fs::read_to_string(rolled_path(&path)).unwrap(),
            "0123456789"
        );
    }

    #[test]
    fn rotate_overwrites_a_previous_dot_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(rolled_path(&path), b"old rotation").unwrap();
        std::fs::write(&path, b"0123456789").unwrap();

        rotate_if_too_large(&path, 10).unwrap();

        assert_eq!(
            std::fs::read_to_string(rolled_path(&path)).unwrap(),
            "0123456789"
        );
    }

    #[test]
    fn rotate_on_missing_file_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.jsonl");
        assert!(rotate_if_too_large(&path, 10).is_ok());
    }
}
