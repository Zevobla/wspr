//! The screen header band: a flush-bottom `<h2>` title on the left with a
//! per-screen trailing slot (tags, search, a primary action) on the right,
//! closed by the system's 2px bottom rule.

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::theme::{color, spacing, type_scale};

/// A screen's header. `trailing` fills the right side (pass an empty
/// element for none). The band is `HEADER_H` tall with the title aligned to
/// its bottom edge, then a 2px rule -- matched to the rail brand's own
/// bottom-weighted position in its `RAIL_HEADER_H` band (`crate::hub`'s
/// `brand`, which adds a baseline compensation for its smaller type) so the
/// two sit on the same baseline. `.align_y(Alignment::End)` on
/// this outer `container` is what actually anchors the row to the band's
/// bottom edge (the row's own `.align_y(End)` below only aligns the title
/// against `trailing`, since both are already the same height); the same
/// pairing is used by `crate::theme::widgets::marks::meter`.
pub fn screen_header<'a, M: 'a>(
    title: impl Into<String>,
    trailing: Element<'a, M>,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let bar = container(
        row![
            text(title.into())
                .size(type_scale::TITLE_LARGE.size)
                .font(type_scale::TITLE_LARGE.font())
                .color(scheme.on_surface),
            Space::new().width(Length::Fill),
            trailing,
        ]
        .align_y(Alignment::End)
        .width(Length::Fill),
    )
    .height(Length::Fixed(spacing::layout::HEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::End)
    .padding(iced::Padding {
        top: 0.0,
        right: spacing::XXL,
        bottom: spacing::layout::HEADER_TITLE_PAD_BOTTOM,
        left: spacing::XXL,
    });

    column![bar, super::hr(scheme)].width(Length::Fill).into()
}
