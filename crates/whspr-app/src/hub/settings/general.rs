//! The "General" section: the language override and the launch-at-login /
//! sound-feedback toggles. The ASR and refiner backend pickers live in the
//! Models tab now (one unified selector each), so they're intentionally not
//! duplicated here.

use iced::widget::{column, pick_list};
use iced::Element;

use crate::config_ui;
use crate::hub::common::{field, section, toggle_row};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let language_picker = pick_list(
        config_ui::LANGUAGE_LABELS,
        Some(config_ui::language_label(&state.config.language)),
        |label: &'static str| Message::LanguageChanged(label.to_string()),
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    section(
        scheme,
        "General",
        column![
            field(
                scheme,
                "Language ('auto' = no override)",
                language_picker.into()
            ),
            toggle_row(
                scheme,
                "Launch at login",
                state.config.autostart.enabled,
                Message::AutostartToggled,
            ),
            toggle_row(
                scheme,
                "Play a sound on start/stop",
                state.config.sound.enabled,
                Message::SoundFeedbackToggled,
            ),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
