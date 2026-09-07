//! The "Audio & devices" group: the input-device picker
//! (`microphone_section`), the `config.device` flag toggles
//! (`flags_section`), and the hotkey preview (`hotkey_section`).

use iced::widget::{button, column, pick_list, row, text};
use iced::{Alignment, Element};

use crate::hub::common::{field, section, toggle_row};
use crate::state::{Message, State};
use crate::theme::widgets;
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
    section(
        scheme,
        "Devices",
        column![
            toggle_row(
                scheme,
                "Rescan devices when one is plugged/unplugged",
                state.config.device.device_hotplug,
                Message::DeviceHotplugToggled,
            ),
            toggle_row(
                scheme,
                "Track the focused app for per-app stats",
                state.config.device.active_window,
                Message::ActiveWindowToggled,
            ),
            toggle_row(
                scheme,
                "Allow Bluetooth microphones",
                state.config.device.bluetooth_source,
                Message::BluetoothSourceToggled,
            ),
            toggle_row(
                scheme,
                "Allow virtual/software audio sources",
                state.config.device.virtual_source,
                Message::VirtualSourceToggled,
            ),
            toggle_row(
                scheme,
                "Keep the tray icon static (no flicker on state changes)",
                state.config.device.tray_static,
                Message::TrayStaticToggled,
            ),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

fn hotkey_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let capture_label = if state.hotkey_capturing {
        "Press any key..."
    } else {
        "Change"
    };
    let capture_button = button(
        text(capture_label)
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::text(scheme, status))
    .on_press(Message::StartHotkeyCapture);

    // The fixed push-to-talk combo rendered as Modernist keycaps.
    let keycaps = row![
        widgets::kbd("Ctrl", scheme),
        widgets::kbd("Space", scheme),
        capture_button,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    let preview: Element<'_, Message> = match &state.captured_hotkey {
        Some(combo) => text(format!("Captured: {combo} (preview only, not yet applied)"))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => text(
            "Ctrl+Space is fixed -- whspr-inject doesn't yet support registering a \
              different combo at runtime.",
        )
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into(),
    };

    section(
        scheme,
        "Hotkey",
        column![field(scheme, "Push to talk", keycaps.into()), preview]
            .spacing(spacing::SM)
            .into(),
    )
}
