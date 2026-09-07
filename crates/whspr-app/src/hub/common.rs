//! Shared layout helpers for the Hub screens, restyled for Modernist:
//! `section` (a flush, titled block on the paper ground -- structure comes
//! from the 18/800 head and the 2px rules screens place between sections,
//! not from a card fill), `card` (the one tinted fill, for the few genuine
//! card uses), `field` (a caption tight above its control), and `kicker`
//! (an uppercase tracked label).

use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};

use crate::state::Message;
use crate::theme::widgets;
use crate::theme::{color, spacing, type_scale};

/// A titled section: an 18/800 head over `body`, flush on the ground (no
/// card fill -- Modernist separates sections with rules and alignment).
pub(super) fn section<'a>(
    scheme: &'static color::Scheme,
    title: &'static str,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        text(title)
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font())
            .color(scheme.on_surface),
        body,
    ]
    .spacing(spacing::MD)
    .width(Length::Fill)
    .into()
}

/// An uppercase tracked kicker ("TRANSCRIPT · LIVE", "RECENT"). The string
/// is uppercased here since iced has no `text-transform`.
pub(super) fn kicker<'a>(
    scheme: &'static color::Scheme,
    label: impl AsRef<str>,
) -> Element<'a, Message> {
    text(label.as_ref().to_uppercase())
        .size(type_scale::KICKER.size)
        .font(type_scale::KICKER.font())
        .color(scheme.primary)
        .into()
}

/// A settings row: a square Modernist toggle switch followed by its label.
/// `on_toggle` is the message constructor (receives the new value).
pub(super) fn toggle_row<'a>(
    scheme: &'static color::Scheme,
    label: &'static str,
    value: bool,
    on_toggle: fn(bool) -> Message,
) -> Element<'a, Message> {
    row![
        widgets::toggle(value, on_toggle, scheme),
        text(label)
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font())
            .color(scheme.on_surface),
    ]
    .spacing(spacing::MD)
    .align_y(Alignment::Center)
    .into()
}

/// A field caption (12px, dimmed) tightly grouped above its control.
pub(super) fn field<'a>(
    scheme: &'static color::Scheme,
    label: &'static str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
        control,
    ]
    .spacing(spacing::XS)
    .into()
}
