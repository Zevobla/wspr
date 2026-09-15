//! The "Add from a link" modal dialog (design comp 2c): a ~800px card
//! centered over a dark scrim, overlaid on the Dictate shell via `stack!` in
//! `crate::hub::view`. Split across submodules so each stays well under the
//! AA-06 line cap: `card` (the resolved media card + captions/transcribe
//! choice), `chapters` (the chapter list, clip inputs, and the playlist /
//! sign-in right column), and `format` (timecode/date helpers).
//!
//! The dialog reads from a `&LinkImport` (its state + handlers live in
//! `crate::link_import`); every control emits a `LinkImport*` message. The
//! backdrop dims but does not dismiss on click -- Cancel is explicit -- so a
//! stray click on the card's empty space can't lose the user's choices.

mod card;
mod chapters;
mod format;

use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::link_import::LinkImport;
use crate::state::Message;
use crate::theme::widgets;
use crate::theme::{color, shape, spacing, styles, type_scale};

/// The dialog's fixed width, from the comp.
const DIALOG_W: f32 = 800.0;
/// The dialog's max height, from the comp -- the body scrolls past it.
const DIALOG_MAX_H: f32 = 668.0;

/// The scrim: ink at 55%, a fixed dark veil in both themes (comp 2c).
const SCRIM: Color = Color::from_rgba(
    0x20 as f32 / 255.0,
    0x1e as f32 / 255.0,
    0x1d as f32 / 255.0,
    0.55,
);

/// The whole modal layer: the dark scrim filling the shell, with the dialog
/// card centered on it. Overlaid via `stack!` in `crate::hub::view`.
pub fn overlay<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(dialog(li, scheme))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(SCRIM)),
            ..container::Style::default()
        })
        .into()
}

/// The dialog card: a fixed-width paper panel -- header, a scrolling body,
/// and the footer -- lifted off the scrim with a soft shadow.
fn dialog<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let card = column![
        header(scheme),
        widgets::hr(scheme),
        url_row(li, scheme),
        resolved_body(li, scheme),
        widgets::hr(scheme),
        footer(li, scheme),
    ]
    .width(Length::Fill);

    container(
        scrollable(card).style(move |_theme, status| styles::scrollable::rail(scheme, status)),
    )
    .width(Length::Fixed(DIALOG_W))
    .max_height(DIALOG_MAX_H)
    .style(move |_theme| container::Style {
        background: Some(iced::Background::Color(scheme.surface)),
        border: Border {
            color: scheme.outline_variant,
            width: spacing::layout::RULE,
            radius: 0.0.into(),
        },
        shadow: iced::Shadow {
            color: Color {
                a: 0.35,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 12.0),
            blur_radius: 40.0,
        },
        ..container::Style::default()
    })
    .into()
}

/// The header band: the title and a muted "yt-dlp · bundled" tag.
fn header<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        row![
            text("Add from a link")
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font())
                .color(scheme.on_surface),
            Space::new().width(Length::Fill),
            text("yt-dlp \u{00b7} bundled")
                .size(type_scale::LABEL_MEDIUM.size)
                .font(type_scale::LABEL_MEDIUM.font())
                .color(scheme.on_surface_variant),
        ]
        .align_y(Alignment::Center),
    )
    .padding([spacing::MD, spacing::XL])
    .width(Length::Fill)
    .into()
}

/// The URL row: a bound text input + a Resolve button, plus any resolve error.
fn url_row<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let input = text_input("Paste a YouTube or media link\u{2026}", &li.url)
        .on_input(Message::LinkImportUrl)
        .on_submit(Message::LinkImportResolve)
        .width(Length::Fill)
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    let resolve_label = if li.resolving {
        "Resolving\u{2026}"
    } else {
        "Resolve"
    };
    let resolve = button(
        text(resolve_label)
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([spacing::SM, spacing::LG])
    .style(move |_theme, status| styles::button::outlined(scheme, status))
    .on_press_maybe((!li.resolving).then_some(Message::LinkImportResolve));

    let field = row![input, resolve]
        .spacing(spacing::MD)
        .align_y(Alignment::Center);

    let error: Element<'a, Message> = match &li.error {
        Some(message) => text(format!("Couldn't resolve: {message}"))
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.primary)
            .into(),
        None => Space::new().into(),
    };

    container(column![field, error].spacing(spacing::SM))
        .padding([spacing::MD, spacing::XL])
        .width(Length::Fill)
        .into()
}

/// The resolved section -- media card, captions/transcribe choice, and the
/// chapters/playlist grid -- shown only after a successful resolve.
fn resolved_body<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let Some(media) = &li.media else {
        return Space::new().into();
    };
    column![
        widgets::hr(scheme),
        card::media_card(media, li.thumbnail.as_ref(), scheme),
        card::choice_selector(li, media, scheme),
        chapters::panel_row(li, media, scheme),
    ]
    .spacing(spacing::LG)
    .padding([0.0, spacing::XL])
    .into()
}

/// The footer: a muted privacy note, Cancel, and the "Open note desk" confirm
/// (disabled until media resolves). The confirm emits `LinkImportConfirm`,
/// which runs the chosen import and opens the note desk (see
/// `crate::link_import::update`).
fn footer<'a>(li: &'a LinkImport, scheme: &'static color::Scheme) -> Element<'a, Message> {
    // While an import runs, the note area becomes the live status (accent): a
    // real whisper progress bar once transcription reports, else just the
    // phase text — so the dialog visibly works instead of looking frozen.
    let status = |content: String, color| {
        text(content)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(color)
            .width(Length::Fill)
    };
    let note: Element<'a, Message> = match (&li.importing, li.import_progress) {
        (Some(phase), Some(percent)) => column![
            progress_bar(0.0..=1.0, percent as f32 / 100.0)
                .girth(Length::Fixed(6.0))
                .style(move |_theme| progress_bar::Style {
                    background: Background::Color(scheme.surface_container_highest),
                    bar: Background::Color(scheme.primary),
                    border: Border::default().rounded(shape::NONE),
                }),
            status(format!("{phase}  \u{00b7}  {percent}%"), scheme.primary),
        ]
        .spacing(spacing::XS)
        .width(Length::Fill)
        .into(),
        (Some(phase), None) => status(phase.clone(), scheme.primary).into(),
        (None, _) => status(
            "Downloads audio only and deletes it after transcribing \u{2014} change in \
             Settings \u{2192} Privacy."
                .to_string(),
            scheme.on_surface_variant,
        )
        .into(),
    };

    let cancel = button(
        text("Cancel")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([spacing::SM, spacing::LG])
    .style(move |_theme, status| styles::button::outlined(scheme, status))
    .on_press(Message::LinkImportCancel);

    let importing = li.importing.is_some();
    // A quieter, outlined sibling to the note-desk confirm: transcribes the
    // audio straight into the Dictate transcript + History instead of building
    // a note (see `crate::link_import::start_transcribe_to_dictate`).
    let transcribe = button(
        text("Transcribe to Dictate \u{2192}")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([spacing::SM, spacing::LG])
    .style(move |_theme, status| styles::button::outlined(scheme, status))
    .on_press_maybe(
        (li.media.is_some() && !importing).then_some(Message::LinkImportTranscribeToDictate),
    );

    let confirm = button(
        text(if importing {
            "Importing\u{2026}"
        } else {
            "Open note desk \u{2192}"
        })
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([spacing::SM, spacing::LG])
    .style(move |_theme, status| styles::button::filled(scheme, status))
    .on_press_maybe((li.media.is_some() && !importing).then_some(Message::LinkImportConfirm));

    container(
        row![note, cancel, transcribe, confirm]
            .spacing(spacing::MD)
            .align_y(Alignment::Center),
    )
    .padding([spacing::MD, spacing::XL])
    .width(Length::Fill)
    .into()
}

/// A 2px-outline square (an unselected radio/checkbox mark), matching the
/// filled `widgets::status_square(Mark::Solid, ..)` at the same size.
pub(super) fn outline_square<'a>(
    size: f32,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme| container::Style {
            border: Border {
                color: scheme.on_surface,
                width: 2.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
