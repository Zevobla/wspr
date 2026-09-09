//! The note desk's transcript column: a 47px header ("TRANSCRIPT" + a static
//! Key/All/Kept segmented control) over the live transcript rows. Each row is
//! a `[time | gutter | text | score]` grid; kept rows carry an inset accent
//! left rule, and a run of rows by one speaker is topped by an uppercase
//! speaker label. Static this phase -- keep/dismiss, reassign and re-rank
//! land in later phases.

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::note_desk::{Gutter, NoteDeskState, TranscriptRow};
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

/// The transcript column: a fixed width, a header band, then the rows.
pub(super) fn view<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    column![header(scheme), widgets::hr(scheme), rows(nd, scheme)]
        .width(Length::Fixed(TRANSCRIPT_W))
        .height(Length::Fill)
        .into()
}

/// The 47px header band: the "TRANSCRIPT" kicker and a static segmented
/// filter (Key active, matching the comp).
fn header<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        row![
            text("TRANSCRIPT")
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(scheme.on_surface_variant)
                .width(Length::Fill),
            seg_control(scheme),
        ]
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(SUBHEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding([0.0, spacing::LG])
    .into()
}

/// A static segmented control (no re-filtering this phase). Mirrors
/// `widgets::segmented`'s look with plain containers so it needs no message.
fn seg_control<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    let opts = [("Key", true), ("All", false), ("Kept", false)];
    let last = opts.len() - 1;
    let mut r = row![].align_y(Alignment::Center);
    for (i, (label, active)) in opts.into_iter().enumerate() {
        let (bg, fg) = if active {
            (Some(Background::Color(scheme.primary)), scheme.on_primary)
        } else {
            (None, scheme.on_surface)
        };
        r = r.push(
            container(
                text(label)
                    .size(type_scale::KICKER.size)
                    .font(type_scale::LABEL_LARGE.font())
                    .color(fg),
            )
            .padding([3.0, 8.0])
            .style(move |_theme| container::Style {
                background: bg,
                ..container::Style::default()
            }),
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

/// The section heading + the transcript rows, with a speaker label above each
/// speaker's run.
fn rows<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut col = column![section_heading(scheme)].width(Length::Fill);
    let mut prev_speaker: Option<&str> = None;
    let last = nd.rows.len().saturating_sub(1);
    for (i, r) in nd.rows.iter().enumerate() {
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
        .height(Length::Fill)
        .padding([0.0, spacing::LG])
        .into()
}

/// A placeholder section heading ("N · Title" + "from MM:SS" + a rule). The
/// transcript rows carry no section field yet, so the label is sample data
/// this phase (real sectioning is a later phase).
fn section_heading<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    row![
        text("2 · Microstates")
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font())
            .color(scheme.on_surface),
        text("from 11:40")
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
}
