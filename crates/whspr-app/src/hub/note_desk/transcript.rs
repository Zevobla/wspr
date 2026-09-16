//! The note desk's transcript column: a 47px header ("TRANSCRIPT" + a static
//! Key/All/Kept segmented control) over the live transcript rows. Each row is
//! a `[time | gutter | text | score]` grid; kept rows carry an inset accent
//! left rule, and a run of rows by one speaker is topped by an uppercase
//! speaker label. Static this phase -- keep/dismiss, reassign and re-rank
//! land in later phases.

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::note_desk::{Gutter, NoteDeskState, TranscriptFilter, TranscriptRow};
use crate::state::Message;
use crate::theme::widgets;
use crate::theme::{color, spacing, styles, type_scale};

use super::{SUBHEADER_H, TRANSCRIPT_W};

/// The time column's width.
const TIME_W: f32 = 42.0;
/// The gutter (keep-mark) column's width.
const GUTTER_W: f32 = 16.0;
/// The keep-score column's width.
const SCORE_W: f32 = 22.0;
/// The gutter square's side.
const MARK: f32 = 13.0;
/// The inset accent left rule's width on kept rows.
const RULE_W: f32 = 3.0;

/// The transcript column: a fixed width, a fixed header band, then the rows
/// in a `scrollable` so a long transcript (hundreds of rows) scrolls within
/// the column instead of overflowing it (the header stays pinned above).
pub(super) fn view<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let body = scrollable(rows(nd, scheme))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme, status| styles::scrollable::rail(scheme, status));
    column![header(nd, scheme), widgets::hr(scheme), body]
        .width(Length::Fixed(TRANSCRIPT_W))
        .height(Length::Fill)
        .into()
}

/// The 47px header band: the "TRANSCRIPT" kicker and the interactive
/// segmented filter (the active option accent-filled, matching the comp).
fn header<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        row![
            text("TRANSCRIPT")
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(scheme.on_surface_variant)
                .width(Length::Fill),
            seg_control(nd.filter, scheme),
        ]
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(SUBHEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding([0.0, spacing::LG])
    .into()
}

/// The `Key / All / Kept` segmented filter: a bordered row of `button`s, the
/// one matching `active` accent-filled, each emitting a `SetTranscriptFilter`.
fn seg_control<'a>(
    active: TranscriptFilter,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let opts = [
        ("Key", TranscriptFilter::Key),
        ("All", TranscriptFilter::All),
        ("Kept", TranscriptFilter::Kept),
    ];
    let last = opts.len() - 1;
    let mut r = row![].align_y(Alignment::Center);
    for (i, (label, filter)) in opts.into_iter().enumerate() {
        let is_active = filter == active;
        r = r.push(
            button(
                text(label)
                    .size(type_scale::KICKER.size)
                    .font(type_scale::LABEL_LARGE.font()),
            )
            .padding([3.0, 8.0])
            .on_press(Message::SetTranscriptFilter(filter))
            .style(move |_theme, status| seg_style(scheme, is_active, status)),
        );
        if i != last {
            r = r.push(
                container(Space::new())
                    .width(Length::Fixed(1.0))
                    .height(Length::Fixed(20.0))
                    .style(move |_theme| styles::container::divider(scheme)),
            );
        }
    }
    container(r)
        .style(move |_theme| container::Style {
            border: Border {
                color: scheme.outline,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One segment's style: the active option is an accent fill with paper text;
/// the rest are transparent with an ink hover/press wash.
fn seg_style(
    scheme: &'static color::Scheme,
    active: bool,
    status: button::Status,
) -> button::Style {
    if active {
        return button::Style {
            background: Some(Background::Color(scheme.primary)),
            text_color: scheme.on_primary,
            border: Border::default().rounded(0.0),
            ..button::Style::default()
        };
    }
    let base = button::Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.07))),
            ..base
        },
        _ => base,
    }
}

/// The section heading + the transcript rows, with a speaker label above each
/// speaker's run.
fn rows<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let visible: Vec<&TranscriptRow> = nd.rows.iter().filter(|&r| nd.filter.keeps(r)).collect();
    let mut col = column![section_heading(nd, scheme)].width(Length::Fill);
    let mut prev_speaker: Option<&str> = None;
    let last = visible.len().saturating_sub(1);
    for (i, r) in visible.into_iter().enumerate() {
        let speaker = r.speaker_id.as_deref();
        if speaker != prev_speaker {
            if let Some(name) = speaker {
                col = col.push(speaker_label(name, scheme));
            }
            prev_speaker = speaker;
        }
        col = col.push(transcript_row(r, scheme));
        if i != last {
            col = col.push(widgets::hairline(scheme));
        }
    }
    container(col)
        .width(Length::Fill)
        .padding([0.0, spacing::LG])
        .into()
}

/// The section heading: the first kept chapter's title + start time + a rule,
/// from the imported [`NoteHeading`]s. Full per-row sectioning is a later
/// phase; with no chapters (e.g. a manual desk) it collapses to a small gap so
/// the transcript doesn't butt against the header rule.
fn section_heading<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let Some(first) = nd.headings.first() else {
        return Space::new().height(Length::Fixed(spacing::MD)).into();
    };
    row![
        text(first.title.clone())
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font())
            .color(scheme.on_surface),
        text(format!("from {}", first.time_label))
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
        widgets::hr(scheme),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .padding([spacing::MD, 0.0])
    .into()
}

/// An uppercase speaker label above a speaker's run of rows.
fn speaker_label<'a>(name: &str, scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        text(name.to_uppercase())
            .size(type_scale::KICKER.size)
            .font(type_scale::KICKER.font())
            .color(color::wash(scheme.on_surface, 0.55)),
    )
    .padding([spacing::SM, 0.0])
    .into()
}

/// One transcript row: an inset accent rule (kept rows only) then the
/// `[time | gutter | text | score]` grid.
fn transcript_row<'a>(
    r: &'a TranscriptRow,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let rule_color = if r.gutter == Gutter::Kept {
        scheme.primary
    } else {
        Color::TRANSPARENT
    };
    let left_rule = container(Space::new())
        .width(Length::Fixed(RULE_W))
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(rule_color)),
            ..container::Style::default()
        });

    let grid = row![
        container(
            text(r.time_label.clone())
                .size(type_scale::LABEL_MEDIUM.size)
                .font(type_scale::LABEL_MEDIUM.font())
                .color(scheme.on_surface_variant),
        )
        .width(Length::Fixed(TIME_W)),
        container(gutter_mark(r.gutter, scheme)).width(Length::Fixed(GUTTER_W)),
        text(r.text.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .line_height(iced::widget::text::LineHeight::Relative(1.4))
            .color(scheme.on_surface)
            .width(Length::Fill),
        score(r.keep_score, scheme),
    ]
    .spacing(9.0)
    .align_y(Alignment::Start)
    .width(Length::Fill);

    container(row![left_rule, grid].width(Length::Fill))
        .padding([spacing::SM, 0.0])
        .width(Length::Fill)
        .into()
}

/// The keep-gutter mark: filled square (Kept), 2px accent-outline square
/// (Candidate), or empty (Chatter).
fn gutter_mark<'a>(g: Gutter, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match g {
        Gutter::Kept => container(Space::new())
            .width(Length::Fixed(MARK))
            .height(Length::Fixed(MARK))
            .style(move |_theme| styles::container::accent(scheme))
            .into(),
        Gutter::Candidate => container(Space::new())
            .width(Length::Fixed(MARK))
            .height(Length::Fixed(MARK))
            .style(move |_theme| container::Style {
                border: Border {
                    color: scheme.primary,
                    width: 2.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into(),
        Gutter::Chatter => Space::new()
            .width(Length::Fixed(MARK))
            .height(Length::Fixed(MARK))
            .into(),
    }
}

/// Up to three 4px keep-score dots: `keep` accent, the rest dim.
fn score<'a>(keep: u8, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut r = row![].spacing(2.0);
    for i in 0..3u8 {
        let dot = if i < keep {
            scheme.primary
        } else {
            color::wash(scheme.on_surface, 0.20)
        };
        r = r.push(
            container(Space::new())
                .width(Length::Fixed(4.0))
                .height(Length::Fixed(4.0))
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(dot)),
                    ..container::Style::default()
                }),
        );
    }
    container(r)
        .width(Length::Fixed(SCORE_W))
        .padding([spacing::XS, 0.0])
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_column_builds() {
        let _: Element<'_, Message> = view(&NoteDeskState::sample(), &color::LIGHT);
    }

    #[test]
    fn gutter_marks_build_for_every_state() {
        for g in [Gutter::Kept, Gutter::Candidate, Gutter::Chatter] {
            let _: Element<'_, Message> = gutter_mark(g, &color::LIGHT);
        }
    }

    #[test]
    fn active_segment_uses_accent_fill() {
        let style = seg_style(&color::LIGHT, true, button::Status::Active);
        assert_eq!(
            style.background,
            Some(Background::Color(color::LIGHT.primary))
        );
        assert_eq!(style.text_color, color::LIGHT.on_primary);
    }

    #[test]
    fn inactive_segment_is_transparent() {
        let style = seg_style(&color::LIGHT, false, button::Status::Active);
        assert!(style.background.is_none());
        assert_eq!(style.text_color, color::LIGHT.on_surface);
    }
}
