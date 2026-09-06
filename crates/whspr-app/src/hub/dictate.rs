//! The Dictate screen: whspr's hero experience. A big, obvious Record
//! affordance (a filled ● button that turns into a red ■ Stop while a
//! capture is live), the fixed-hotkey hint, a prominent live input-level
//! meter, the recognized-transcript box, and the secondary Copy /
//! Transcribe-a-file actions -- the old `transcribe_section` reworked to be
//! the centerpiece a user lands on rather than one card among settings.

use iced::widget::{button, column, container, progress_bar, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::section;

/// Whether the Copy action has anything to copy: `true` only when there's a
/// non-blank transcript on screen. Pure so the enabled/disabled logic is
/// testable without building the button, and shared by `view` both to gate
/// the `on_press` and (implicitly) to render the disabled style.
fn copy_enabled(state: &State) -> bool {
    state
        .transcribed_text
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty())
}

/// Renders the Dictate screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let hero = column![
        record_button(state.is_recording, scheme),
        hint(scheme),
        level_meter(state, scheme),
        status(state, scheme),
    ]
    .spacing(spacing::LG)
    .align_x(Alignment::Center)
    .width(Length::Fill);

    section(
        scheme,
        "Dictate",
        column![hero, transcript(state, scheme), actions(state, scheme)]
            .spacing(spacing::XL)
            .into(),
    )
}

/// The primary Record affordance: a large filled button reading "● Record"
/// when idle, flipping to a red "■ Stop recording" while a capture is live.
fn record_button(recording: bool, scheme: &'static color::Scheme) -> Element<'static, Message> {
    let label = if recording {
        "\u{25A0}  Stop recording"
    } else {
        "\u{25CF}  Record"
    };

    button(
        text(label)
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font()),
    )
    .padding([spacing::MD, spacing::XL])
    .style(move |_theme, s| {
        if recording {
            styles::button::error(scheme, s)
        } else {
            styles::button::filled(scheme, s)
        }
    })
    .on_press(Message::ToggleRecording)
    .into()
}

/// The fixed-hotkey hint under the Record button.
fn hint(scheme: &'static color::Scheme) -> Element<'static, Message> {
    text("Hold Ctrl+Space to dictate into any app")
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}

/// The live input-level meter: a chunky, always-present bar that fills with
/// the mic RMS (scaled up for visibility) while recording and sits empty
/// otherwise, so it reads as a real level meter rather than the thin
/// progress indicator it used to be.
fn level_meter(state: &State, scheme: &'static color::Scheme) -> Element<'static, Message> {
    let level = if state.is_recording {
        (state.mic_level * 4.0).min(1.0)
    } else {
        0.0
    };

    progress_bar(0.0..=1.0, level)
        .length(Length::Fixed(360.0))
        .girth(Length::Fixed(12.0))
        .style(move |_theme| styles::progress_bar::thinking(scheme))
        .into()
}

/// The in-flight/just-finished status line (e.g. "Transcribing...").
fn status<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match &state.transcribe_status {
        Some(s) => text(s.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => column![].into(),
    }
}

/// The recognized-transcript box: the last run's text on a tonal surface,
/// or a guiding placeholder when there's nothing to show yet.
fn transcript<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let inner: Element<'_, Message> = match &state.transcribed_text {
        Some(t) if !t.trim().is_empty() => scrollable(
            text(t.clone())
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface),
        )
        .height(Length::Fixed(140.0))
        .style(move |_theme, s| styles::scrollable::rail(scheme, s))
        .into(),
        _ => text("Record from the mic or pick a .wav file to see the transcript here.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    container(inner).width(Length::Fill).into()
}

/// The secondary action row: Copy (disabled until there's a transcript) and
/// Transcribe-a-file.
fn actions<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let copy = button(
        text("Copy")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, s| styles::button::tonal(scheme, s))
    .on_press_maybe(copy_enabled(state).then_some(Message::CopyTranscript));

    let pick = button(
        text("Transcribe a file...")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, s| styles::button::outlined(scheme, s))
    .on_press(Message::PickFileToTranscribe);

    row![copy, pick].spacing(spacing::MD).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_config::Config;

    fn state_with_transcript(text: Option<&str>) -> State {
        let mut state = State::new(Config::default());
        state.transcribed_text = text.map(|t| t.to_string());
        state
    }

    #[test]
    fn copy_disabled_without_a_transcript() {
        assert!(!copy_enabled(&state_with_transcript(None)));
    }

    #[test]
    fn copy_disabled_for_a_blank_transcript() {
        assert!(!copy_enabled(&state_with_transcript(Some("   "))));
    }

    #[test]
    fn copy_enabled_with_a_real_transcript() {
        assert!(copy_enabled(&state_with_transcript(Some("hello world"))));
    }
}
