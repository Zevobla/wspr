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

/// A chip row: "No sign-in" plus each **installed** browser (detected by
/// [`whspr_import::installed_cookie_browsers`]), the active one filled.
fn browser_picker<'a>(
    current: Option<&str>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mut chips = row![chip("No sign-in", None, current.is_none(), scheme)]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
    for browser in whspr_import::installed_cookie_browsers() {
        let whspr_import::CookieBrowser { label, spec } = browser;
        let selected = current == Some(spec.as_str());
        chips = chips.push(chip(&label, Some(spec), selected, scheme));
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
