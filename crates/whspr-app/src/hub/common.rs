//! Shared layout helpers used by every Hub screen: `section` (an M3 "card")
//! and `field` (a caption grouped tightly above its control). Pulled out of
//! `hub::mod` so each screen module (`dictate`, `speakers`, `history`,
//! `settings`) can reach for the same building blocks without duplicating
//! them.

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::state::Message;
use crate::theme::{color, spacing, styles, type_scale};

/// A settings-panel "card": a title in `TITLE_MEDIUM` over `body`, tonal
/// background from `styles::container::section`.
pub(super) fn section<'a>(
    scheme: &'static color::Scheme,
    title: &'static str,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font())
                .color(scheme.on_surface),
            body,
        ]
        .spacing(spacing::MD),
    )
    .padding(spacing::LG)
    .width(Length::Fill)
    .style(move |_theme| styles::container::section(scheme))
    .into()
}

/// A field caption (`LABEL_MEDIUM`, de-emphasized) tightly grouped above
/// its control -- MD3's "tight groups, generous separation" spacing rule.
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
