//! The "General" section: the language override and the launch-at-login /
//! sound-feedback toggles. The ASR and refiner backend pickers live in the
//! Models tab now -- one unified selector each, listing local files and cloud
//! backends together (see `crate::hub::models`) -- so they're intentionally
//! not duplicated here.

use iced::widget::{checkbox, column, pick_list};
use iced::Element;

use crate::config_ui;
use crate::hub::common::{field, section};
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

    let autostart_toggle: Element<'_, Message> = checkbox(state.config.autostart.enabled)
        .label("Launch at login")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::AutostartToggled)
        .into();

    let sound_toggle: Element<'_, Message> = checkbox(state.config.sound.enabled)
        .label("Play a sound on start/stop")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::SoundFeedbackToggled)
        .into();

    section(
        scheme,
        "General",
        column![
            field(
                scheme,
                "Language ('auto' = no override)",
                language_picker.into()
            ),
            autostart_toggle,
            sound_toggle,
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
