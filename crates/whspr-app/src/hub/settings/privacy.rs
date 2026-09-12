//! The "Privacy" section: microphone-privacy and history-encryption toggles
//! (AG-01, AG-03, M-17), plus the media-import sign-in browser picker.

use iced::widget::{button, column, row, text};
use iced::{Alignment, Element};

use crate::hub::common::{field, section, toggle_row};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    section(
        scheme,
        "Privacy",
        column![
            toggle_row(
                scheme,
                "Release the microphone when not recording",
                state.config.privacy.mic_privacy,
                Message::MicPrivacyToggled,
            ),
            toggle_row(
                scheme,
                "Encrypt history at rest",
                state.config.privacy.history_encryption,
                Message::HistoryEncryptionToggled,
            ),
            field(
                scheme,
                "Media-import sign-in \u{2014} borrow a signed-in browser for \
                 bot-walled or private videos",
                browser_picker(state.config.privacy.cookies_browser.as_deref(), scheme),
            ),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

/// The `yt-dlp --cookies-from-browser` sources offered per platform, as
/// `(id, display label)`. Firefox and Chrome carry a real signed-in session
/// (they defeat the YouTube bot wall); Safari on macOS is offered for
/// completeness. yt-dlp errors clearly for a browser that isn't installed.
fn cookie_browsers() -> &'static [(&'static str, &'static str)] {
    #[cfg(target_os = "windows")]
    {
        &[("chrome", "Chrome"), ("firefox", "Firefox"), ("edge", "Edge")]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            ("firefox", "Firefox"),
            ("chrome", "Chrome"),
            ("safari", "Safari"),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        &[
            ("firefox", "Firefox"),
            ("chrome", "Chrome"),
            ("chromium", "Chromium"),
        ]
    }
}

/// A chip row: "No sign-in" plus each available browser, the active one filled.
fn browser_picker<'a>(
    current: Option<&str>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mut chips = row![chip("No sign-in", None, current.is_none(), scheme)]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
    for (id, label) in cookie_browsers() {
        chips = chips.push(chip(
            label,
            Some((*id).to_string()),
            current == Some(*id),
            scheme,
        ));
    }
    chips.into()
}

/// One browser chip (or "No sign-in" when `browser` is `None`): filled when
/// active, outlined otherwise.
fn chip<'a>(
    label: &str,
    browser: Option<String>,
    selected: bool,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([spacing::XS, spacing::MD])
    .style(move |_theme, status| {
        if selected {
            styles::button::filled(scheme, status)
        } else {
            styles::button::outlined(scheme, status)
        }
    })
    .on_press(Message::CookieBrowserChanged(browser))
    .into()
}
