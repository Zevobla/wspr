//! Env-gated headless screenshot path (a dev/verification aid).
//!
//! Full-screen OS capture needs screen-recording permission and is
//! unreliable in CI/headless contexts, so instead: set
//! `WHSPR_SCREENSHOT=<path>` and whspr-app captures just the Hub window to a
//! clean PNG once it has rendered, then exits 0. iced's own
//! `window::screenshot` returns the composited RGBA of a single window (no
//! OS permission), which `save` encodes with the pure-Rust `png` crate.
//!
//! This is a real, exercised code path -- the subscription and handlers are
//! always compiled and wired; they simply no-op unless the env var is set --
//! so it carries no dead code (AC-06).

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::window::Screenshot;

use crate::state::{Message, State};

/// The screenshot output path from `WHSPR_SCREENSHOT`, if set.
pub fn path_from_env() -> Option<PathBuf> {
    std::env::var_os("WHSPR_SCREENSHOT").map(PathBuf::from)
}

/// Once, ~800ms after start (enough for fonts to load and the first frame
/// to composite), asks for the capture. Inert unless a path was requested
/// and the shot hasn't been taken yet, so it stops firing after the one
/// capture and costs nothing in normal runs.
pub fn subscription(state: &State) -> iced::Subscription<Message> {
    if state.screenshot_path.is_some() && !state.screenshot_taken {
        iced::time::every(Duration::from_millis(800)).map(|_| Message::TakeScreenshot)
    } else {
        iced::Subscription::none()
    }
}

/// Encodes a captured window screenshot (RGBA8, physical pixels) to a PNG
/// at `path`.
pub fn save(path: &Path, shot: &Screenshot) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, shot.size.width, shot.size.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    writer
        .write_image_data(&shot.rgba)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}
