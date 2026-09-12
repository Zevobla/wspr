//! The resolved-media card and the captions/transcribe choice for the
//! link-import dialog (comp 2c): a thumbnail placeholder + title/meta/tags,
//! then the two-choice selector. Remote thumbnails are out of scope, so the
//! card shows a labelled grey placeholder box rather than fetching an image.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length};

use whspr_import::MediaInfo;

use super::format::{human_date, mmss};
use super::outline_square;
use crate::link_import::LinkImport;
use crate::state::Message;
use crate::theme::widgets::{self, Mark, TagKind};
use crate::theme::{color, spacing, type_scale};

/// The grey thumbnail placeholder's size (comp: 150x72).
const THUMB_W: f32 = 150.0;
const THUMB_H: f32 = 72.0;

/// The resolved-media card: a thumbnail placeholder beside the title, a muted
/// `uploader · duration · date` line, and the availability tags.
pub fn media_card<'a>(
    media: &'a MediaInfo,
    thumb: Option<&'a iced::widget::image::Handle>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    row![
        thumbnail(thumb, scheme),
        column![
            text(media.title.clone())
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font())
                .color(scheme.on_surface),
            meta_line(media, scheme),
            tags(media, scheme),
        ]
        .spacing(spacing::SM)
        .width(Length::Fill),
    ]
    .spacing(spacing::LG)
    .align_y(Alignment::Start)
    .into()
}

/// The media thumbnail: the fetched image when available, else a grey
/// "Thumbnail" placeholder box.
fn thumbnail<'a>(
    thumb: Option<&'a iced::widget::image::Handle>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    if let Some(handle) = thumb {
        return iced::widget::Image::new(handle.clone())
            .width(Length::Fixed(THUMB_W))
            .height(Length::Fixed(THUMB_H))
            .content_fit(iced::ContentFit::Contain)
            .into();
    }
    container(
        text("Thumbnail")
            .size(type_scale::KICKER.size)
            .font(type_scale::KICKER.font())
            .color(scheme.surface),
    )
    .width(Length::Fixed(THUMB_W))
    .height(Length::Fixed(THUMB_H))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(scheme.neutral[4])),
        ..container::Style::default()
    })
    .into()
}

/// The muted `uploader · duration · date` line; absent fields are skipped.
fn meta_line<'a>(media: &'a MediaInfo, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(uploader) = &media.uploader {
        parts.push(uploader.clone());
    }
    if let Some(duration) = media.duration_secs {
        parts.push(mmss(duration));
    }
    if let Some(date) = &media.upload_date {
        parts.push(human_date(date));
    }
    text(parts.join(" \u{00b7} "))
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}

/// The availability tags: caption tracks (human/auto), chapter count, and a
/// playlist marker -- matching the comp's outline/neutral tag row.
fn tags<'a>(media: &'a MediaInfo, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut tag_row = row![].spacing(spacing::SM);
    let mut captioned = false;
    for lang in &media.human_captions {
        let label = if captioned {
            format!("{} (human)", lang.code)
        } else {
            captioned = true;
            format!("Captions: {} (human)", lang.code)
        };
        tag_row = tag_row.push(widgets::tag(TagKind::Outline, label, scheme));
    }
    for lang in &media.auto_captions {
        let label = if captioned {
            format!("{} (auto)", lang.code)
        } else {
            captioned = true;
            format!("Captions: {} (auto)", lang.code)
        };
        tag_row = tag_row.push(widgets::tag(TagKind::Outline, label, scheme));
    }
    if !media.chapters.is_empty() {
        tag_row = tag_row.push(widgets::tag(
            TagKind::Neutral,
            format!("{} chapters", media.chapters.len()),
            scheme,
        ));
    }
    if let Some(playlist) = &media.playlist {
        tag_row = tag_row.push(widgets::tag(
            TagKind::Neutral,
            format!("Playlist \u{00b7} {} videos", playlist.count),
            scheme,
        ));
    }
    tag_row.into()
}

/// The two-choice selector: "Use published captions" vs "Transcribe here".
/// The active option carries an accent 2px border; each shows a square mark
/// (filled accent when selected, 2px outline otherwise) and a sub-line.
pub fn choice_selector<'a>(
    li: &'a LinkImport,
    media: &'a MediaInfo,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let captions_available = !media.human_captions.is_empty() || !media.auto_captions.is_empty();
    let caption_sub = if !captions_available {
        "No captions for this video".to_string()
    } else {
        match &li.caption_lang {
            Some(code) => format!("{code} \u{00b7} already timestamped, no model needed"),
            None => "already timestamped \u{00b7} no model needed".to_string(),
        }
    };

    let captions = choice_card(
        "Use published captions",
        &caption_sub,
        li.use_captions,
        captions_available.then_some(Message::LinkImportUseCaptions(true)),
        scheme,
    );
    let transcribe = choice_card(
        "Transcribe here",
        "whisper \u{00b7} speaker labels",
        !li.use_captions,
        Some(Message::LinkImportUseCaptions(false)),
        scheme,
    );

    row![captions, transcribe]
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
}

/// One choice box: a square mark + bold heading over a muted sub-line, in a
/// full-width bordered button (accent border when `selected`).
fn choice_card<'a>(
    heading: &str,
    sub: &str,
    selected: bool,
    on_press: Option<Message>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mark: Element<'a, Message> = if selected {
        widgets::status_square(Mark::Solid, 12.0, scheme)
    } else {
        outline_square(12.0, scheme)
    };
    let head = row![
        mark,
        text(heading.to_string())
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font())
            .color(scheme.on_surface),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    let body = column![
        head,
        text(sub.to_string())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
    ]
    .spacing(spacing::XS);

    button(body)
        .width(Length::Fill)
        .padding([spacing::SM, spacing::MD])
        .style(move |_theme, status| choice_style(selected, scheme, status))
        .on_press_maybe(on_press)
        .into()
}

/// A choice box's button style: transparent with a 2px border (accent when
/// selected, divider otherwise) and a faint ink wash on hover.
fn choice_style(selected: bool, scheme: &color::Scheme, status: button::Status) -> button::Style {
    let border_color = if selected {
        scheme.primary
    } else {
        scheme.outline_variant
    };
    let base = button::Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border {
            color: border_color,
            width: 2.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    };
    match status {
        button::Status::Disabled => button::Style {
            text_color: scheme.on_surface_variant,
            ..base
        },
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.05))),
            ..base
        },
        _ => base,
    }
}
