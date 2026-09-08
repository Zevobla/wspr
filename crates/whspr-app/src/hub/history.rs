//! The History screen: a search box, a stat strip (dictations / words /
//! average speed), and a filtered table of past transcriptions -- restyled
//! onto the Modernist widgets.

use iced::widget::{column, row, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::history::HistoryEntry;
use crate::state::{Message, State};
use crate::stats;
use crate::theme::widgets::{self};
use crate::theme::{color, icons, spacing, styles, type_scale};

use super::common::kicker;

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Shown in the Speaker column when a dictation carries no attribution
/// (fingerprinting off, no model, or the embedding failed).
const SPEAKER_UNATTRIBUTED: &str = "—";

/// Resolves the Speaker column label for one history entry against the
/// enrolled-speaker db. Pure over `(entry, db)` so it's unit-testable
/// without a running app:
/// - a matching profile with a user-set `name` -> that name;
/// - a `speaker_id` with no name (or no matching profile) -> the UUID,
///   shortened to its first group so it fits the column but still reads as
///   the speaker's id;
/// - no `speaker_id` -> a neutral placeholder.
fn speaker_label(entry: &HistoryEntry, db: &whspr_config::SpeakerDb) -> String {
    match &entry.speaker_id {
        None => SPEAKER_UNATTRIBUTED.to_string(),
        Some(id) => db
            .profiles
            .iter()
            .find(|p| &p.id == id)
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| short_uuid(id)),
    }
}

/// The first group of a UUID (its first 8 characters) -- enough to
/// disambiguate a speaker in a fixed-width column while staying legible.
/// Shorter ids are returned whole.
fn short_uuid(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Renders the History screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        search_row(state, scheme),
        stat_strip(state, scheme),
        widgets::hr(scheme),
        history_table(state, scheme),
    ]
    .spacing(spacing::XL)
    .width(Length::Fill)
    .into()
}

/// The search box, with a leading icon and (when non-empty) a Clear action.
fn search_row<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let input = text_input("Search transcripts", &state.history_search)
        .on_input(Message::HistorySearchChanged)
        .padding([6.0, 10.0])
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    let mut r = row![
        icons::icon(icons::SEARCH, 14.0, scheme.on_surface_variant),
        input,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    if !state.history_search.is_empty() {
        r = r.push(
            iced::widget::button(
                text("Clear")
                    .size(type_scale::LABEL_LARGE.size)
                    .font(type_scale::LABEL_LARGE.font()),
            )
            .style(move |_theme, s| styles::button::text(scheme, s))
            .on_press(Message::HistorySearchChanged(String::new())),
        );
    }

    r.into()
}

/// The four-cell stat strip.
fn stat_strip<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let count = state.history.len();
    let words: usize = state.history.iter().map(|e| word_count(&e.text)).sum();
    let wpm = stats::average_wpm(&state.history)
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "--".to_string());

    row![
        stat_cell(scheme, &count.to_string(), "Dictations"),
        widgets::vrule(spacing::layout::HAIRLINE, scheme),
        stat_cell(scheme, &words.to_string(), "Words"),
        widgets::vrule(spacing::layout::HAIRLINE, scheme),
        stat_cell(scheme, &wpm, "Avg wpm"),
    ]
    .spacing(spacing::XL)
    .height(Length::Fixed(72.0))
    .into()
}

fn stat_cell<'a>(
    scheme: &'static color::Scheme,
    number: &str,
    label: &'static str,
) -> Element<'a, Message> {
    column![
        text(number.to_string())
            .size(type_scale::STAT.size)
            .font(type_scale::STAT.font())
            .color(scheme.on_surface),
        kicker(scheme, label),
    ]
    .spacing(spacing::XS)
    .width(Length::FillPortion(1))
    .into()
}

/// The transcription table, filtered by the search query.
fn history_table<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let query = state.history_search.to_lowercase();
    let rows: Vec<Vec<Element<'a, Message>>> = state
        .history
        .iter()
        .rev()
        .filter(|e| query.is_empty() || e.text.to_lowercase().contains(&query))
        .map(|entry| {
            vec![
                text(entry.text.clone())
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface)
                    .into(),
                text(speaker_label(entry, &state.speaker_db))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface_variant)
                    .into(),
                text(format!("{} words", word_count(&entry.text)))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface_variant)
                    .into(),
            ]
        })
        .collect();

    if rows.is_empty() {
        let msg = if state.history.is_empty() {
            "No transcriptions yet -- dictate something to see it here."
        } else {
            "Nothing matches your search."
        };
        return column![
            text(msg)
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
            Space::new(),
        ]
        .into();
    }

    widgets::table(
        vec![
            ("What you said", Length::Fill),
            ("Speaker", Length::Fixed(160.0)),
            ("Words", Length::Fixed(96.0)),
        ],
        rows,
        scheme,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_config::{SpeakerDb, SpeakerProfile};

    const UUID: &str = "1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d";

    fn profile(id: &str, name: Option<&str>) -> SpeakerProfile {
        SpeakerProfile {
            id: id.to_string(),
            name: name.map(str::to_string),
            centroid: Vec::new(),
            samples: 0,
            scans: Vec::new(),
            first_seen: 0,
            last_seen: 0,
        }
    }

    fn entry(speaker_id: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            text: "hello world".to_string(),
            duration_secs: None,
            speaker_id: speaker_id.map(str::to_string),
        }
    }

    #[test]
    fn word_count_counts_tokens() {
        assert_eq!(word_count("a b c"), 3);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn speaker_label_uses_the_profile_name_when_set() {
        let db = SpeakerDb {
            profiles: vec![profile(UUID, Some("Ada"))],
        };
        assert_eq!(speaker_label(&entry(Some(UUID)), &db), "Ada");
    }

    #[test]
    fn speaker_label_falls_back_to_the_short_uuid_when_unnamed() {
        let db = SpeakerDb {
            profiles: vec![profile(UUID, None)],
        };
        assert_eq!(speaker_label(&entry(Some(UUID)), &db), "1a2b3c4d");
    }

    #[test]
    fn speaker_label_shows_the_short_uuid_when_no_profile_matches() {
        let db = SpeakerDb::default();
        assert_eq!(speaker_label(&entry(Some(UUID)), &db), "1a2b3c4d");
    }

    #[test]
    fn speaker_label_shows_a_placeholder_when_unattributed() {
        let db = SpeakerDb::default();
        assert_eq!(speaker_label(&entry(None), &db), SPEAKER_UNATTRIBUTED);
    }
}
