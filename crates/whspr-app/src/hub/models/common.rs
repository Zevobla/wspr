//! Shared widgets for the Models screen: body/label text, the theme-aware
//! fit tag, the catalog table, and the download/delete action buttons.
//! Pulled out of the section modules (`asr`, `refine`, `dirs`) so they
//! don't each redefine the same helpers.

use std::path::PathBuf;

use iced::widget::{button, row, text};
use iced::{Alignment, Element, Length};

use crate::state::Message;
use crate::theme::widgets::{self, TagKind};
use crate::theme::{color, icons, spacing, styles, type_scale};

/// A `BODY_MEDIUM`, de-emphasized paragraph -- the wording used across this
/// screen's status/help lines.
pub(super) fn body_text(
    content: String,
    scheme: &'static color::Scheme,
) -> Element<'static, Message> {
    text(content)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}

/// A `LABEL_LARGE` button label, matching the other screens' buttons.
pub(super) fn label_text(content: &'static str) -> Element<'static, Message> {
    text(content)
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font())
        .into()
}

/// The "fits your machine" verdict as a theme-aware Modernist tag: a calm
/// neutral tag for green (fits), an accent-tinted tag for yellow (tight),
/// and an accent-outline tag for red (too large). All three read on both
/// the light and dark grounds because `widgets::tag` draws from the
/// scheme's neutral/accent ramps (which invert per theme).
pub(super) fn fit_tag(
    fit: whspr_hf::Fit,
    scheme: &'static color::Scheme,
) -> Element<'static, Message> {
    let kind = match fit {
        whspr_hf::Fit::Green => TagKind::Neutral,
        whspr_hf::Fit::Yellow => TagKind::Accent,
        whspr_hf::Fit::Red => TagKind::Outline,
    };
    widgets::tag(kind, fit.label(), scheme)
}

/// One catalog row: the model name, its on-disk size, the fit verdict, and
/// the trailing action (download or delete).
pub(super) struct CatalogRow<'a> {
    pub name: String,
    pub size_bytes: u64,
    pub fit: whspr_hf::Fit,
    pub action: Element<'a, Message>,
}

/// The model catalog as a Modernist `table`: Model / Size / Fit / action.
pub(super) fn catalog_table<'a>(
    scheme: &'static color::Scheme,
    rows: Vec<CatalogRow<'a>>,
) -> Element<'a, Message> {
    let cells: Vec<Vec<Element<'a, Message>>> = rows
        .into_iter()
        .map(|r| {
            vec![
                text(r.name)
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface)
                    .into(),
                text(whspr_hf::human_size(r.size_bytes))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface_variant)
                    .into(),
                fit_tag(r.fit, scheme),
                r.action,
            ]
        })
        .collect();

    widgets::table(
        vec![
            ("Model", Length::Fill),
            ("Size", Length::Fixed(96.0)),
            ("Fit", Length::Fixed(150.0)),
            ("", Length::Fixed(140.0)),
        ],
        cells,
        scheme,
    )
}

/// A "Download" button (with a download icon) that fires `message` unless a
/// background op is busy.
pub(super) fn download_button(
    scheme: &'static color::Scheme,
    busy: bool,
    message: Message,
) -> Element<'static, Message> {
    button(
        row![
            icons::icon(icons::DOWNLOAD, 14.0, scheme.on_secondary_container),
            label_text("Download"),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .style(move |_theme, status| styles::button::tonal(scheme, status))
    .on_press_maybe((!busy).then_some(message))
    .into()
}

/// A "Delete" button (with a trash icon) that removes the model file at
/// `path` unless busy.
pub(super) fn delete_button(
    scheme: &'static color::Scheme,
    busy: bool,
    path: PathBuf,
) -> Element<'static, Message> {
    button(
        row![
            icons::icon(icons::TRASH, 14.0, scheme.on_surface),
            label_text("Delete"),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .style(move |_theme, status| styles::button::outlined(scheme, status))
    .on_press_maybe((!busy).then_some(Message::HfDeleteModel(path)))
    .into()
}
