//! Env-gated headless screenshot path -- how the installer's design is
//! self-validated on macOS without a Windows box.
//!
//! Set `WHSPR_SETUP_SCREENSHOT=<path>` (output PNG) and
//! `WHSPR_SETUP_SCREEN=<install|install-expanded|installing|done|failure>`
//! (which screen); ~800ms after the first frame composites, the window is
//! captured with iced's own `window::screenshot` (composited RGBA, no OS
//! screen-recording permission) and encoded to `path` with the pure-Rust
//! `png` crate, then the process exits 0. This mirrors `whspr-app`'s
//! `crate::screenshot`. It's always compiled and wired; it simply no-ops
//! unless the env var is set, so it carries no dead code.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::window::Screenshot;

use crate::state::{Message, State};

/// The screenshot output path from `WHSPR_SETUP_SCREENSHOT`, if set.
pub fn path_from_env() -> Option<PathBuf> {
    std::env::var_os("WHSPR_SETUP_SCREENSHOT").map(PathBuf::from)
}

/// Once, ~800ms after start (enough for the Archivo faces to load and the
/// first frame to composite), asks for the capture. Inert unless a path was
/// requested and the shot hasn't been taken yet, so it stops firing after
/// the one capture and costs nothing in normal runs.
pub fn subscription(state: &State) -> iced::Subscription<Message> {
    if state.screenshot_path.is_some() && !state.screenshot_taken {
        iced::time::every(Duration::from_millis(800)).map(|_| Message::TakeScreenshot)
    } else {
        iced::Subscription::none()
    }
}

/// Encodes a captured window screenshot (RGBA8, physical pixels) to a PNG at
/// `path`.
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
