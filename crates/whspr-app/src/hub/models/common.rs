//! Small shared widgets for the Models screen: body/label text, the "fits
//! your machine" pill, a per-model management row, and the download/delete
//! action buttons. Pulled out of the section modules (`asr`, `refine`,
//! `dirs`) so they don't each redefine the same helpers.

use std::path::PathBuf;

use iced::widget::{button, container, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length};

use crate::state::Message;
use crate::theme::{color, shape, spacing, styles, type_scale};

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

/// A small colored "fits your machine" pill for a [`whspr_hf::Fit`] verdict:
/// green (`success_container`), yellow (`tertiary_container`), red
/// (`error_container`), reusing the scheme's tonal container roles.
pub(super) fn fit_badge(
    fit: whspr_hf::Fit,
    scheme: &'static color::Scheme,
) -> Element<'static, Message> {
    let (bg, fg) = match fit {
        whspr_hf::Fit::Green => (scheme.success_container, scheme.on_success_container),
        whspr_hf::Fit::Yellow => (scheme.tertiary_container, scheme.on_tertiary_container),
        whspr_hf::Fit::Red => (scheme.error_container, scheme.on_error_container),
    };

    container(
        text(fit.label())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(fg),
    )
    .padding(spacing::XS)
    .style(move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: Border::default().rounded(shape::SM),
        ..Default::default()
    })
    .into()
}

/// One row in a model-management table: name, size, fit badge, and a
/// right-aligned action (a download or delete button).
pub(super) fn manage_row<'a>(
    scheme: &'static color::Scheme,
    name: String,
    size_bytes: u64,
    fit: whspr_hf::Fit,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    let name = text(name)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface)
        .width(Length::Fixed(240.0));

    let size = text(whspr_hf::human_size(size_bytes))
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .width(Length::Fixed(90.0));

    row![
        name,
        size,
        fit_badge(fit, scheme),
        Space::new().width(Length::Fill),
        action,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// A "Download" button that fires `message` unless a background op is busy.
pub(super) fn download_button(
    scheme: &'static color::Scheme,
    busy: bool,
    message: Message,
) -> Element<'static, Message> {
    button(label_text("Download"))
        .style(move |_theme, status| styles::button::tonal(scheme, status))
        .on_press_maybe((!busy).then_some(message))
        .into()
}

/// A "Delete" button that removes the model file at `path` unless busy.
pub(super) fn delete_button(
    scheme: &'static color::Scheme,
    busy: bool,
    path: PathBuf,
) -> Element<'static, Message> {
    button(label_text("Delete"))
        .style(move |_theme, status| styles::button::outlined(scheme, status))
        .on_press_maybe((!busy).then_some(Message::HfDeleteModel(path)))
        .into()
}
