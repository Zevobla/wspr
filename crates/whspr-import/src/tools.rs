//! Discovery of the external command-line tools this crate drives.
//!
//! whspr is bring-your-own-tools for import: it doesn't compile `yt-dlp` or
//! `ffmpeg` in, it shells out to them. [`resolve_tool`] finds each one with a
//! fixed precedence that mirrors `whspr_asr::WhisperLocal::resolve_model_path`:
//!
//! 1. an explicit **environment override** (`WHSPR_YTDLP` / `WHSPR_FFMPEG`) —
//!    an escape hatch pointing at a specific binary, trusted as given;
//! 2. a **bundled** copy at `<exe_dir>/../Resources/<tool>` — the macOS
//!    `.app` layout, and the contract a later app-bundling phase satisfies;
//! 3. whatever is on **`PATH`**.
//!
//! Returns `None` if the tool is nowhere to be found, so callers can surface
//! a clear "install yt-dlp" error rather than spawning a path that isn't there.

use std::path::{Path, PathBuf};

/// An external command-line tool whspr-import orchestrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// The `yt-dlp` media downloader / metadata extractor.
    YtDlp,
    /// The `ffmpeg` transcoder (used to produce a 16kHz mono WAV).
    Ffmpeg,
}

impl Tool {
    /// The tool's executable file name (also its name inside `Resources/`).
    pub fn binary_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
        }
    }

    /// The environment variable that overrides discovery for this tool.
    pub fn env_var(self) -> &'static str {
        match self {
            Tool::YtDlp => "WHSPR_YTDLP",
            Tool::Ffmpeg => "WHSPR_FFMPEG",
        }
    }
}

/// Locates a [`Tool`], honoring (in order) its env override, a bundled copy
/// next to the running executable, then `PATH`. See the module docs for the
/// full contract.
pub fn resolve_tool(tool: Tool) -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(tool.env_var()) {
        return Some(PathBuf::from(explicit));
    }
    if let Some(bundled) = bundled_path(tool) {
        return Some(bundled);
    }
    find_on_path(tool.binary_name())
}

/// The bundled location for `tool` inside a macOS `.app`: a sibling
/// `Resources/` directory next to the `MacOS/` folder the executable lives
/// in (`<exe_dir>/../Resources/<tool>`). Returns `None` if the current
/// executable path is unknown or the file isn't actually there.
fn bundled_path(tool: Tool) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidate = exe_dir
        .join("..")
        .join("Resources")
        .join(tool.binary_name());
    candidate.exists().then_some(candidate)
}

/// Scans the `PATH` environment variable for an executable named `name`,
/// returning the first match. Implemented by hand (via `std::env::split_paths`,
/// which is correct on every platform) rather than pulling in a `which`
/// crate for one directory walk.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_file(candidate))
}

/// Whether `p` names an existing regular file (or a symlink to one).
fn is_file(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_and_env_names_are_stable() {
        assert_eq!(Tool::YtDlp.binary_name(), "yt-dlp");
        assert_eq!(Tool::Ffmpeg.binary_name(), "ffmpeg");
        assert_eq!(Tool::YtDlp.env_var(), "WHSPR_YTDLP");
        assert_eq!(Tool::Ffmpeg.env_var(), "WHSPR_FFMPEG");
    }

    /// The env override wins and is returned verbatim (trusted as given,
    /// like `resolve_model_path`), for both tools. Both are exercised in one
    /// test because `cargo test` runs tests in parallel threads and mutating
    /// a process-wide env var from several at once would race.
    #[test]
    fn env_override_takes_precedence() {
        std::env::set_var("WHSPR_YTDLP", "/opt/custom/yt-dlp");
        std::env::set_var("WHSPR_FFMPEG", "/opt/custom/ffmpeg");

        assert_eq!(
            resolve_tool(Tool::YtDlp),
            Some(PathBuf::from("/opt/custom/yt-dlp")),
            "WHSPR_YTDLP should override discovery and be returned verbatim"
        );
        assert_eq!(
            resolve_tool(Tool::Ffmpeg),
            Some(PathBuf::from("/opt/custom/ffmpeg")),
            "WHSPR_FFMPEG should override discovery and be returned verbatim"
        );

        std::env::remove_var("WHSPR_YTDLP");
        std::env::remove_var("WHSPR_FFMPEG");
    }
}
