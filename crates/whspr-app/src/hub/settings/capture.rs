//! The "Capture" section: audio-capture and transcript-handling toggles --
//! noise suppression, input gain, voice-activity sensitivity, translate/
//! shorten, and the auto-send/input-field-detection/refine-timeout knobs
//! that used to only be reachable by hand-editing `config.toml`'s
//! `[capture]` table.

use iced::widget::{column, slider, text, text_input};
use iced::Element;

use crate::hub::common::{field, section, toggle_row};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    section(
        scheme,
        "Capture",
        column![
            toggle_row(
                scheme,
                "Suppress background noise",
                state.config.capture.noise_suppression,
                Message::NoiseSuppressionToggled,
            ),
            input_gain_field(state, scheme),
            vad_threshold_field(state, scheme),
            toggle_row(
                scheme,
                "Translate to English",
                state.config.capture.translate,
                Message::TranslateToggled,
            ),
            toggle_row(
                scheme,
                "Shorten the transcript",
                state.config.capture.shorten,
                Message::ShortenToggled,
            ),
            toggle_row(
                scheme,
                "Auto-send when recording pauses",
                state.config.capture.auto_send,
                Message::AutoSendToggled,
            ),
            toggle_row(
                scheme,
                "Detect input fields before injecting",
                state.config.capture.input_field_detection,
                Message::InputFieldDetectionToggled,
            ),
            refine_timeout_field(state, scheme),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

fn input_gain_field<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let gain = state.config.capture.input_gain;
    field(
        scheme,
        "Input gain",
        column![
            text(format!("{gain:.2}x"))
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
            slider(0.0..=3.0, gain, Message::InputGainChanged)
                .step(0.05_f32)
                .style(move |_theme, status| styles::slider::field(scheme, status)),
        ]
        .spacing(spacing::XS)
        .into(),
    )
}

fn vad_threshold_field<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let threshold = state.config.capture.vad_threshold;
    field(
        scheme,
        "Voice-activity threshold",
        column![
            text(format!("{threshold:.2}"))
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
            slider(0.0..=1.0, threshold, Message::VadThresholdChanged)
                .step(0.01_f32)
                .style(move |_theme, status| styles::slider::field(scheme, status)),
        ]
        .spacing(spacing::XS)
        .into(),
    )
}

fn refine_timeout_field<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let input = text_input("30000", &state.refine_timeout_draft)
        .on_input(Message::RefineTimeoutMsChanged)
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    field(scheme, "Refine timeout (ms)", input.into())
}
