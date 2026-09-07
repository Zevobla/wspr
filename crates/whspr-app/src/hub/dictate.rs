//! The Dictate screen: whspr's hero. A record/stop block with a live level
//! meter, the recognized transcript, the Copy / Transcribe-a-file actions,
//! and a Recent table -- restyled onto the Modernist widgets (flat, flush
//! left, 2px rules between blocks).

use iced::widget::{button, column, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::state::{Message, Screen, State};
use crate::theme::widgets::{self, Mark, TagKind};
use crate::theme::{color, icons, spacing, styles, type_scale};

use super::common::kicker;

/// Whether the Copy action has anything to copy.
fn copy_enabled(state: &State) -> bool {
    state
        .transcribed_text
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty())
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Renders the Dictate screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        hero(state, scheme),
        widgets::hr(scheme),
        transcript_block(state, scheme),
        widgets::hr(scheme),
        recent_block(state, scheme),
    ]
    .spacing(spacing::XL)
    .width(Length::Fill)
    .into()
}

/// The record/stop block: a live status + meter on the left, the primary
/// action on the right.
fn hero<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let left: Element<'a, Message> = if state.is_recording {
        let level = (state.mic_level * 4.0).min(1.0);
        column![
            row![
                widgets::status_square(Mark::Solid, 10.0, scheme),
                kicker(scheme, "Listening · release to type it in"),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
            widgets::meter(level, 18, 56.0, scheme),
        ]
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
    } else {
        column![
            row![
                widgets::tag(TagKind::Neutral, "Auto-detect · EN", scheme),
                widgets::tag(TagKind::Neutral, "Local LLM cleanup", scheme),
            ]
            .spacing(spacing::SM),
            text("Hold Ctrl+Space to dictate into any app, or record here.")
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
        ]
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
    };

    row![left, record_button(state.is_recording, scheme)]
    .spacing(spacing::XL)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// The primary Record/Stop button (accent fill, icon + label).
fn record_button<'a>(recording: bool, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let (glyph, label): (&'static [u8], &str) = if recording {
        (icons::SQUARE, "Stop")
    } else {
        (icons::MIC, "Record")
    };
    button(
        row![
            icons::icon(glyph, 16.0, scheme.on_primary),
            text(label.to_string())
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font()),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .padding([spacing::MD, spacing::XL])
    .style(move |_theme, s| styles::button::filled(scheme, s))
    .on_press(Message::ToggleRecording)
    .into()
}

/// The transcript block: a kicker + word count, the transcript body, and
/// the Copy / Transcribe-a-file actions.
fn transcript_block<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let words = state
        .transcribed_text
        .as_deref()
        .map(word_count)
        .unwrap_or(0);

    let header = row![
        kicker(scheme, "Transcript · live"),
        Space::new().width(Length::Fill),
        text(format!("{words} words"))
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
    ]
    .align_y(Alignment::Center);

    let body: Element<'a, Message> = match &state.transcribed_text {
        Some(t) if !t.trim().is_empty() => text(t.clone())
            .size(type_scale::TRANSCRIPT.size)
            .font(type_scale::TRANSCRIPT.font())
            .line_height(iced::widget::text::LineHeight::Relative(1.4))
            .color(scheme.on_surface)
            .into(),
        _ => text("Record from the mic or pick a .wav file to see the transcript here.")
            .size(type_scale::TRANSCRIPT.size)
            .font(type_scale::TRANSCRIPT.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    let status: Element<'a, Message> = match &state.transcribe_status {
        Some(s) => text(s.clone())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => Space::new().into(),
    };

    column![header, body, actions(state, scheme), status]
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
}

/// Copy (secondary, gated) and Transcribe-a-file (ghost).
fn actions<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let copy = button(
        row![
            icons::icon(icons::CLIPBOARD, 14.0, scheme.on_surface),
            text("Copy")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, s| styles::button::outlined(scheme, s))
    .on_press_maybe(copy_enabled(state).then_some(Message::CopyTranscript));

    let pick = button(
        row![
            icons::icon(icons::FILE_TEXT, 14.0, scheme.primary),
            text("Transcribe a file...")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, s| styles::button::text(scheme, s))
    .on_press(Message::PickFileToTranscribe);

    row![copy, pick].spacing(spacing::MD).into()
}

/// The Recent block: a kicker + "All history" link, and the last three
/// dictations in a table.
fn recent_block<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let header = row![
        kicker(scheme, "Recent"),
        Space::new().width(Length::Fill),
        button(
            text("All history \u{2192}")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        )
        .style(move |_theme, s| styles::button::text(scheme, s))
        .on_press(Message::TabSelected(Screen::History)),
    ]
    .align_y(Alignment::Center);

    let rows: Vec<Vec<Element<'a, Message>>> = state
        .history
        .iter()
        .rev()
        .take(3)
        .map(|entry| {
            let words = word_count(&entry.text);
            vec![
                text(truncate(&entry.text, 72))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface)
                    .into(),
                text(format!("{words} words"))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface_variant)
                    .into(),
            ]
        })
        .collect();

    let body: Element<'a, Message> = if rows.is_empty() {
        text("Nothing dictated yet.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into()
    } else {
        widgets::table(
            vec![
                ("What you said", Length::Fill),
                ("Words", Length::Fixed(96.0)),
            ],
            rows,
            scheme,
        )
    };

    column![header, body].spacing(spacing::MD).width(Length::Fill).into()
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('\u{2026}');
        out
    }
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

    #[test]
    fn word_count_counts_whitespace_separated_tokens() {
        assert_eq!(word_count("hello there world"), 3);
        assert_eq!(word_count("   "), 0);
    }

    #[test]
    fn truncate_adds_ellipsis_past_the_limit() {
        assert_eq!(truncate("abcdef", 3), "abc\u{2026}");
        assert_eq!(truncate("ab", 3), "ab");
    }
}
