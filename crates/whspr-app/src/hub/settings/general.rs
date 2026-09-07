//! The "General" section: ASR/refiner backend pickers, the language
//! override, and the launch-at-login/sound-feedback toggles -- the handful
//! of settings that were editable before the Settings screen was split
//! into per-topic groups (see `super`'s module doc comment).

use iced::widget::{checkbox, column, pick_list};
use iced::Element;

use crate::config_ui::{self, ASR_LABELS, REFINE_LABELS};
use crate::hub::common::{field, section};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let asr_picker = pick_list(
        ASR_LABELS,
        Some(config_ui::asr_label(state.config.asr)),
        Message::AsrSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let refine_picker = pick_list(
        REFINE_LABELS,
        Some(config_ui::refine_label(state.config.refine)),
        Message::RefineSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

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
            field(scheme, "ASR backend", asr_picker.into()),
            field(scheme, "Refiner", refine_picker.into()),
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
