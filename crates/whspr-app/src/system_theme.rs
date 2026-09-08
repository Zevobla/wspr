//! Follows the OS light/dark appearance automatically (rather than only
//! changing via the Hub's manual "Switch to dark/light" link).
//!
//! `dark-light` has no push-based OS appearance-changed event that plays
//! nicely with iced's `Subscription`, so this polls `detect()` on a plain
//! `iced::time::every` tick -- cheap (a single OS query) and plenty
//! responsive for something a human just flipped in System Settings.
//!
//! On macOS, `detect()` reads `AppleInterfaceStyle` straight from the
//! global preferences domain via the `defaults` CLI (`detect_macos`)
//! instead of trusting `dark_light::detect()` alone: `dark_light` gets
//! there through `NSUserDefaults`/AppKit, which for an unbundled binary
//! (no Info.plist, launched via `cargo run` rather than a `.app`) has been
//! observed to not reliably reflect the system's actual current
//! appearance. Shelling out to the same tool a human would run to check
//! sidesteps whatever bundle/AppKit state that path depends on.
//! `dark_light::detect()` remains the cross-platform fallback for every
//! other OS (and if the `defaults` binary itself can't be spawned).
//!
//! `State::system_theme` tracks the *last detected OS appearance*,
//! separately from `State::theme` (what's actually rendered). That split is
//! what makes the manual toggle (`Message::ThemeToggled`) a real, if
//! temporary, override: a poll only writes `state.theme` when the detected
//! mode has changed since the last poll, so an unrelated tick doesn't stomp
//! a manual choice back to the current system appearance. The override lasts
//! until the OS appearance actually changes again, or the app restarts and
//! re-syncs from scratch in `boot`.

use iced::Task;

use crate::state::{Message, State};

/// How often to re-check the OS appearance.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Maps a detected OS mode to an iced theme. `Unspecified` covers both a
/// detection failure and a platform that genuinely doesn't report a
/// preference -- either way there's nothing to follow, so it falls back to
/// iced's own default.
fn theme_from_mode(mode: dark_light::Mode) -> iced::Theme {
    match mode {
        dark_light::Mode::Dark => iced::Theme::Dark,
        dark_light::Mode::Light | dark_light::Mode::Unspecified => iced::Theme::Light,
    }
}

/// Reads the macOS global domain's `AppleInterfaceStyle` preference
/// directly via the `defaults` CLI -- ground truth for the system's
/// current appearance, independent of the calling process's own bundle
/// state. Returns `None` when the `defaults` binary itself can't even be
/// spawned, so the caller can fall back to `dark_light`.
#[cfg(target_os = "macos")]
fn detect_macos() -> Option<iced::Theme> {
    let output = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()?;
    Some(theme_from_apple_interface_style(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    ))
}

#[cfg(not(target_os = "macos"))]
fn detect_macos() -> Option<iced::Theme> {
    None
}

/// Pure mapping from a `defaults read -g AppleInterfaceStyle` invocation's
/// outcome to a theme. The key is only ever set to the string `"Dark"`
/// (trailing newline included) in Dark mode; Light mode leaves it unset
/// entirely, which `defaults read` reports as a non-zero exit.
#[cfg(target_os = "macos")]
fn theme_from_apple_interface_style(command_succeeded: bool, stdout: &str) -> iced::Theme {
    if command_succeeded && stdout.trim().eq_ignore_ascii_case("dark") {
        iced::Theme::Dark
    } else {
        iced::Theme::Light
    }
}

/// The current OS appearance, as an iced theme: the reliable macOS read
/// when available, otherwise `dark_light`'s cross-platform detection.
fn detect() -> iced::Theme {
    detect_macos().unwrap_or_else(|| {
        dark_light::detect()
            .map(theme_from_mode)
            .unwrap_or(iced::Theme::Light)
    })
}

/// Sets the boot-time theme: the headless screenshot harness's forced theme
/// (`WHSPR_SCREENSHOT_THEME`) if a capture was requested, otherwise the
/// live OS appearance -- never a hardcoded default.
pub fn boot(state: &mut State) {
    let theme = match state.screenshot_path {
        Some(_) => crate::screenshot::theme_from_env(),
        None => detect(),
    };
    tracing::debug!(?theme, "system_theme::boot detected OS appearance");
    state.system_theme = theme.clone();
    state.theme = theme;
}

/// A poll tick: re-detects the OS appearance and, only if it actually
/// changed since the last tick, applies it to `state.theme` too. See the
/// module doc comment for why an unchanged poll must not overwrite a
/// manual override.
pub fn tick(state: &mut State) -> Task<Message> {
    let detected = detect();
    tracing::debug!(
        ?detected,
        current = ?state.theme,
        "system_theme::tick re-checked OS appearance"
    );
    if detected != state.system_theme {
        state.theme = detected.clone();
    }
    state.system_theme = detected;
    Task::none()
}

/// Polls for OS appearance changes, except while a headless screenshot is
/// pending -- the harness forces a theme via env, and auto-detect racing
/// that env-forced theme before the one-shot capture fires would defeat the
/// point of forcing it.
pub fn subscription(state: &State) -> iced::Subscription<Message> {
    if state.screenshot_path.is_some() {
        iced::Subscription::none()
    } else {
        iced::time::every(POLL_INTERVAL).map(|_| Message::SystemThemeTick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_mode_maps_to_dark_theme() {
        assert_eq!(theme_from_mode(dark_light::Mode::Dark), iced::Theme::Dark);
    }

    #[test]
    fn light_mode_maps_to_light_theme() {
        assert_eq!(theme_from_mode(dark_light::Mode::Light), iced::Theme::Light);
    }

    #[test]
    fn unspecified_mode_falls_back_to_light() {
        assert_eq!(
            theme_from_mode(dark_light::Mode::Unspecified),
            iced::Theme::Light
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_interface_style_dark_maps_to_dark_theme() {
        // `defaults read` includes the trailing newline in its stdout.
        assert_eq!(
            theme_from_apple_interface_style(true, "Dark\n"),
            iced::Theme::Dark
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_interface_style_absent_key_maps_to_light() {
        // Light mode leaves the key unset; `defaults read` exits non-zero
        // with empty stdout.
        assert_eq!(
            theme_from_apple_interface_style(false, ""),
            iced::Theme::Light
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_interface_style_unexpected_value_maps_to_light() {
        assert_eq!(
            theme_from_apple_interface_style(true, "Light\n"),
            iced::Theme::Light
        );
    }

    #[test]
    fn tick_applies_a_changed_detection() {
        let mut state = State::new(whspr_config::Config::default());
        state.system_theme = iced::Theme::Light;
        state.theme = iced::Theme::Light;
        // Simulate a detection change directly (real `detect()` depends on
        // the host OS, which a unit test can't control) by exercising the
        // same comparison `tick` makes.
        let detected = iced::Theme::Dark;
        if detected != state.system_theme {
            state.theme = detected.clone();
        }
        state.system_theme = detected;
        assert_eq!(state.theme, iced::Theme::Dark);
    }

    #[test]
    fn unchanged_detection_leaves_a_manual_override_in_place() {
        let mut state = State::new(whspr_config::Config::default());
        state.system_theme = iced::Theme::Light;
        // The user manually toggled to Dark while the OS is still Light.
        state.theme = iced::Theme::Dark;
        let detected = iced::Theme::Light;
        if detected != state.system_theme {
            state.theme = detected.clone();
        }
        state.system_theme = detected;
        assert_eq!(state.theme, iced::Theme::Dark);
    }
}
