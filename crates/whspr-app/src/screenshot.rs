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

use crate::state::{Message, Screen, State};

/// The screenshot output path from `WHSPR_SCREENSHOT`, if set.
pub fn path_from_env() -> Option<PathBuf> {
    std::env::var_os("WHSPR_SCREENSHOT").map(PathBuf::from)
}

/// When a capture was requested, selects the surface `WHSPR_SCREENSHOT_SCREEN`
/// asks for so any surface can be shot headlessly; a no-op in normal runs. The
/// note desk isn't a `Screen` variant (it replaces the whole shell), so
/// `note-desk` seeds `State::note_desk` instead, which the Hub view
/// short-circuits into (see `crate::hub::view`).
pub fn apply_to_screen(state: &mut State) {
    if state.screenshot_path.is_some() {
        if screen_is_note_desk() {
            state.note_desk = Some(crate::note_desk::NoteDeskState::sample());
        } else {
            state.screen = screen_from_env();
        }
    }
}

/// Whether `WHSPR_SCREENSHOT_SCREEN` asks for the note desk.
fn screen_is_note_desk() -> bool {
    matches!(
        std::env::var("WHSPR_SCREENSHOT_SCREEN")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "note-desk" | "notedesk" | "note_desk"
    )
}

/// Which screen to show in the capture, from `WHSPR_SCREENSHOT_SCREEN`
/// (`dictate`/`history`/`models`/`speakers`/`settings`). Defaults to
/// `Dictate` when unset or unrecognized.
pub fn screen_from_env() -> Screen {
    match std::env::var("WHSPR_SCREENSHOT_SCREEN")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "history" => Screen::History,
        "models" => Screen::Models,
        "speakers" => Screen::Speakers,
        "settings" => Screen::Settings,
        _ => Screen::Dictate,
    }
}

/// Which theme to capture in, from `WHSPR_SCREENSHOT_THEME` (`light`/
/// `dark`). Defaults to light.
pub fn theme_from_env() -> iced::Theme {
    match std::env::var("WHSPR_SCREENSHOT_THEME")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "dark" => iced::Theme::Dark,
        _ => iced::Theme::Light,
    }
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
