//! The Settings screen: ASR/refiner/language/launch-at-login/sound
//! (`settings_section`), the input-device picker (`device_section`), and
//! the hotkey preview (`hotkey_section`) -- everything that used to be the
//! Hub's first three cards, now grouped behind the "Settings" tab.

use iced::widget::{button, checkbox, column, pick_list, text};
use iced::Element;

use crate::config_ui::{self, ASR_LABELS, REFINE_LABELS};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::{field, section};

/// Renders the Settings screen: backend/language/toggle settings, the
/// microphone device picker, and the hotkey preview, stacked in the same
/// order they used to appear in the old single-column Hub.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        settings_section(state, scheme),
        device_section(state, scheme),
        hotkey_section(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}

fn device_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let device_picker = pick_list(
        state.input_devices.clone(),
        state.selected_device.clone(),
        Message::DeviceSelected,
    )
    .placeholder("No input devices found")
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    section(
        scheme,
        "Microphone",
        field(scheme, "Input device", device_picker.into()),
    )
}

fn hotkey_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let capture_label = if state.hotkey_capturing {
        "Press any key..."
    } else {
        "Preview a new hotkey"
    };
    let capture_button = button(
        text(capture_label)
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::tonal(scheme, status))
    .on_press(Message::StartHotkeyCapture);

    let preview: Element<'_, Message> = match &state.captured_hotkey {
        Some(combo) => text(format!("Captured: {combo} (preview only, not yet applied)"))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => text("No preview captured yet")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    section(
        scheme,
        "Hotkey",
        column![
            text(
                "Active hotkey: Ctrl+Space (fixed -- whspr-inject doesn't yet support \
                  registering a different combo at runtime)"
            )
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant),
            capture_button,
            preview,
        ]
        .spacing(spacing::SM)
        .into(),
    )
}

fn settings_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
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
        "Settings",
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
