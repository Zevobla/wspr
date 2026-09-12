//! The chapter list, clip inputs, and the playlist / sign-in right column for
//! the link-import dialog (comp 2c). Chapters become note headings: each row
//! is an include toggle, a start timecode, the title, and its duration.

use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Alignment, Element, Length};

use whspr_import::{Chapter, MediaInfo, Playlist};

use super::format::{long_duration, mmss, total_minutes};
use super::outline_square;
use crate::hub::common::kicker;
use crate::link_import::LinkImport;
use crate::state::Message;
use crate::theme::widgets::{self, Mark};
use crate::theme::{color, spacing, styles, type_scale};

/// The lower grid: the chapters column (left, fills) beside the playlist /
/// sign-in column (right, fixed), split by a 2px rule.
pub fn panel_row<'a>(
    li: &'a LinkImport,
    media: &'a MediaInfo,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    row![
        chapters_column(li, media, scheme),
        widgets::vrule(spacing::layout::RULE, scheme),
        right_column(li, media, scheme),
    ]
    .spacing(spacing::LG)
    .align_y(Alignment::Start)
    .width(Length::Fill)
    .into()
}

/// The chapters column: a header with the included/total count, the toggle
/// rows, and the "Or clip" inputs.
fn chapters_column<'a>(
    li: &'a LinkImport,
    media: &'a MediaInfo,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let total = media.chapters.len();
    let included = li.chapters_included.iter().filter(|&&on| on).count();
    let included_secs: f32 = media
        .chapters
        .iter()
        .zip(li.chapters_included.iter())
        .filter(|(_, &on)| on)
        .map(|(c, _)| (c.end_secs - c.start_secs).max(0.0))
        .sum();

    let head = row![
        kicker(scheme, "Chapters \u{2192} note headings"),
        Space::new().width(Length::Fill),
        text(format!(
            "{included} of {total} \u{00b7} {}",
            total_minutes(included_secs)
        ))
        .size(type_scale::LABEL_MEDIUM.size)
        .font(type_scale::LABEL_MEDIUM.font())
        .color(scheme.on_surface_variant),
    ]
    .align_y(Alignment::Center);

    let mut list = column![].spacing(0);
    if media.chapters.is_empty() {
        list = list.push(
            text("No chapters in this media.")
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
        );
    }
    for (i, chapter) in media.chapters.iter().enumerate() {
        let on = li.chapters_included.get(i).copied().unwrap_or(false);
        list = list.push(chapter_row(i, chapter, on, scheme));
    }

    column![head, list, clip_row(li, scheme)]
        .spacing(spacing::SM)
        .width(Length::Fill)
        .into()
}

/// One chapter row (a full-width toggle button): an include mark, the start
/// timecode, the title, and the chapter's duration.
fn chapter_row<'a>(
    index: usize,
    chapter: &'a Chapter,
    included: bool,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mark: Element<'a, Message> = if included {
        widgets::status_square(Mark::Solid, 13.0, scheme)
    } else {
        outline_square(13.0, scheme)
    };
    let content = row![
        mark,
        container(
            text(mmss(chapter.start_secs))
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface_variant),
        )
        .width(Length::Fixed(52.0)),
        text(chapter.title.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface)
            .width(Length::Fill),
        text(mmss((chapter.end_secs - chapter.start_secs).max(0.0)))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    button(content)
        .width(Length::Fill)
        .padding([spacing::XS, 0.0])
        .style(move |_theme, status| crate::hub::ghost_row(scheme, status))
        .on_press(Message::LinkImportToggleChapter(index))
        .into()
}

/// The "Or clip [MM:SS] -> [MM:SS]" pair of small inputs.
fn clip_row<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let start = text_input("MM:SS", &li.clip_start)
        .on_input(Message::LinkImportClipStart)
        .width(Length::Fixed(78.0))
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));
    let end = text_input("MM:SS", &li.clip_end)
        .on_input(Message::LinkImportClipEnd)
        .width(Length::Fixed(78.0))
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));
    row![
        muted(scheme, "Or clip"),
        start,
        muted(scheme, "\u{2192}"),
        end,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// The right column: the playlist block (when present) over the sign-in
/// (cookie-borrow) block, in a fixed 232px width.
fn right_column<'a>(
    li: &'a LinkImport,
    media: &'a MediaInfo,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mut col = column![].spacing(spacing::LG).width(Length::Fixed(232.0));
    if let Some(playlist) = &media.playlist {
        col = col.push(playlist_block(playlist, scheme));
    }
    col = col.push(signin_block(li, scheme));
    col.into()
}

/// "Rest of the playlist": a summary line and a (stubbed this phase) "Queue
/// all as one course" button.
fn playlist_block<'a>(
    playlist: &'a Playlist,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let more = playlist.count.saturating_sub(1);
    let total_secs: f32 = playlist
        .entries
        .iter()
        .filter_map(|e| e.duration_secs)
        .sum();
    let summary = if total_secs > 0.0 {
        format!("{more} more videos, {} total.", long_duration(total_secs))
    } else {
        format!("{more} more videos.")
    };
    // Stubbed this phase: the button has no action yet, so it renders inert.
    // A later stream wires "queue the whole playlist as one course".
    let queue = button(
        text("Queue all as one course")
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .width(Length::Fill)
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, status| styles::button::outlined(scheme, status));

    column![
        kicker(scheme, "Rest of the playlist"),
        muted(scheme, summary),
        queue,
    ]
    .spacing(spacing::SM)
    .into()
}

/// "Sign-in": borrow cookies from a browser for members-only / private media.
fn signin_block<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let browser = button(
        text("Safari \u{25be}")
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .width(Length::Fill)
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, status| styles::button::text(scheme, status))
    .on_press(Message::LinkImportBorrowCookies("safari".to_string()));

    let mut col = column![
        kicker(scheme, "Sign-in"),
        muted(
            scheme,
            "Members-only or private? Borrow cookies from a browser.",
        ),
        browser,
    ]
    .spacing(spacing::SM);

    if let Some(name) = &li.cookies_browser {
        col = col.push(
            text(format!("Using {name} cookies."))
                .size(type_scale::LABEL_MEDIUM.size)
                .font(type_scale::LABEL_MEDIUM.font())
                .color(scheme.primary),
        );
    }
    col.into()
}

/// A muted 12px line -- the dialog's help/summary text.
fn muted<'a>(scheme: &'static color::Scheme, label: impl Into<String>) -> Element<'a, Message> {
    text(label.into())
        .size(type_scale::LABEL_MEDIUM.size)
        .font(type_scale::LABEL_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}
