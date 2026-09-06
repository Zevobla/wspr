//! Size-based log rotation (O-19): installs a global `tracing` subscriber
//! writing to a rotating file in the platform data dir, so the app's
//! existing `tracing::warn!`/`tracing::error!` call sites (previously
//! going nowhere -- nothing installed a subscriber) land somewhere
//! durable instead of being silently dropped.
//!
//! Best-effort, like `crate::history`/`crate::speakers`' data-dir
//! helpers: if the data dir can't be determined, falls back to stderr
//! rather than failing to start the app -- logging is diagnostic, not
//! load-bearing.

use std::sync::Mutex;

use file_rotate::compression::Compression;
use file_rotate::suffix::AppendCount;
use file_rotate::{ContentLimit, FileRotate};

/// Rotate the active log file once it reaches this size.
const MAX_LOG_BYTES: usize = 5 * 1024 * 1024;
/// Keep this many rotated files in addition to the active one.
const MAX_ROTATED_FILES: usize = 3;

/// Installs the global `tracing` subscriber. Call once, at startup,
/// before anything else logs -- a second call would fail to install
/// (there can only be one global default subscriber) and is ignored.
pub fn init() {
    match log_file_path() {
        Some(path) => {
            let writer = FileRotate::new(
                path,
                AppendCount::new(MAX_ROTATED_FILES),
                ContentLimit::Bytes(MAX_LOG_BYTES),
                Compression::None,
                None,
            );
            let _ = tracing_subscriber::fmt()
                .with_writer(Mutex::new(writer))
                .with_ansi(false)
                .try_init();
        }
        None => {
            // No writable data dir found -- fall back to stderr so
            // logging still works, just without rotation.
            let _ = tracing_subscriber::fmt().try_init();
        }
    }
}

/// The whspr log file's path in the platform data dir, if determinable on
/// this platform. Mirrors `crate::history::history_file_path`.
fn log_file_path() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "whspr")?;
    Some(dirs.data_dir().join("whspr.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_file_path_is_named_whspr_log() {
        let Some(path) = log_file_path() else {
            // No home/data dir resolvable in this environment -- nothing
            // to assert, and that's the documented fallback behavior.
            return;
        };
        assert_eq!(path.file_name().unwrap(), "whspr.log");
    }
}
