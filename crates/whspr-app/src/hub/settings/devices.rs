//! The "Devices" group: the input-device picker (`microphone_section`), the
//! `config.device` flag toggles (`flags_section`), and the hotkey preview
//! (`hotkey_section`) -- hardware- and input-adjacent settings kept
//! together the way they already were before the Settings screen was split
//! into per-topic groups (see `super`'s module doc comment).

use iced::widget::{button, checkbox, column, pick_list, text};
use iced::Element;

use crate::hub::common::{field, section};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        microphone_section(state, scheme),
        flags_section(state, scheme),
        hotkey_section(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}

fn microphone_section<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
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

fn flags_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let device_hotplug = checkbox(state.config.device.device_hotplug)
        .label("Rescan devices when one is plugged/unplugged")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::DeviceHotplugToggled);

    let active_window = checkbox(state.config.device.active_window)
        .label("Track the focused app for per-app stats")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::ActiveWindowToggled);

    let bluetooth_source = checkbox(state.config.device.bluetooth_source)
        .label("Allow Bluetooth microphones")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::BluetoothSourceToggled);

    let virtual_source = checkbox(state.config.device.virtual_source)
        .label("Allow virtual/software audio sources")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::VirtualSourceToggled);

    let tray_static = checkbox(state.config.device.tray_static)
        .label("Keep the tray icon static (no flicker on state changes)")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::TrayStaticToggled);

    section(
        scheme,
        "Devices",
        column![
            device_hotplug,
            active_window,
            bluetooth_source,
            virtual_source,
            tray_static,
        ]
        .spacing(spacing::MD)
        .into(),
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
