//! The Dictate screen: whspr's core action -- record/stop, a live transcript
//! of the last run, and transcribing an arbitrary file.

use iced::widget::{button, column, progress_bar, row, scrollable, text};
use iced::{Element, Length};

use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::section;

/// Renders the Dictate screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let pick_button = button(
        text("Transcribe a file...")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::filled(scheme, status))
    .on_press(Message::PickFileToTranscribe);

    let recording = state.is_recording;
    let record_button = button(
        text(if recording {
            "Stop recording"
        } else {
            "Record"
        })
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| {
        if recording {
            styles::button::filled(scheme, status)
        } else {
            styles::button::tonal(scheme, status)
        }
    })
    .on_press(Message::ToggleRecording);

    let controls = row![pick_button, record_button].spacing(spacing::MD);

    // Live input-level meter while recording (RMS scaled up for visibility).
    let meter: Element<'_, Message> = if recording {
        progress_bar(0.0..=1.0, (state.mic_level * 4.0).min(1.0))
            .style(move |_theme| styles::progress_bar::thinking(scheme))
            .into()
    } else {
        column![].into()
    };

    let status: Element<'_, Message> = match &state.transcribe_status {
        Some(status) => text(status.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => column![].into(),
    };

    let recognized: Element<'_, Message> = match &state.transcribed_text {
        Some(t) if !t.trim().is_empty() => scrollable(
            text(t.clone())
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface),
        )
        .height(Length::Fixed(120.0))
        .style(move |_theme, status| styles::scrollable::rail(scheme, status))
        .into(),
        Some(_) => text("(empty transcript)")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => text("Record from the mic or pick a .wav file to transcribe it on-screen.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    section(
        scheme,
        "Transcribe & record",
        column![controls, meter, status, recognized]
            .spacing(spacing::MD)
            .into(),
    )
}
