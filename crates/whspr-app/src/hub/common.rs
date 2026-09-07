//! Shared layout helpers for the Hub screens, restyled for Modernist:
//! `section` (a flush, titled block on the paper ground -- structure comes
//! from the 18/800 head and the 2px rules screens place between sections,
//! not from a card fill), `card` (the one tinted fill, for the few genuine
//! card uses), `field` (a caption tight above its control), and `kicker`
//! (an uppercase tracked label).

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::state::Message;
use crate::theme::{color, spacing, styles, type_scale};

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

/// The one tinted fill, zero radius -- for the genuine card uses (onboarding
/// columns, the machine-readout strip). Most Hub content is flush on the
/// ground; reach for this only when a filled block is called for.
pub(super) fn card<'a>(
    scheme: &'static color::Scheme,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    container(body)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| styles::container::card(scheme))
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
