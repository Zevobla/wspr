//! The Failure screen -- a write couldn't complete. A paper header with a
//! "Stopped at 62%" status and a 2px **accent** rule (not the neutral
//! divider) over a centered block: the "Couldn't finish." title, a
//! reassuring line, the error code, and the primary "Retry" button.
//!
//! Reachable via `WHSPR_SETUP_SCREEN=failure`; once the real install lands it
//! is also where a genuine write error routes (see `crate::begin_install`).

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::state::Message;
use crate::theme;
use crate::widgets;

pub fn view() -> Element<'static, Message> {
    let center = column![
        text("Couldn\u{2019}t finish.")
            .size(44.0)
            .font(theme::extrabold())
            .color(theme::INK),
        Space::new().height(Length::Fixed(22.0)),
        text(
            "A file couldn\u{2019}t be written to your account folder. Nothing was left \
             behind \u{2014} retrying is safe."
        )
        .size(16.0)
        .font(theme::regular())
        .color(theme::INK),
        Space::new().height(Length::Fixed(14.0)),
        text("ERROR 0X80070005 \u{00B7} COPYING FILES")
            .size(12.0)
            .font(theme::semibold())
            .color(theme::DIMMED),
        Space::new().height(Length::Fixed(28.0)),
        widgets::primary_button("Retry", Message::Retry),
    ]
    .width(Length::Fill);

    let trailing = row![
        super::header_status("STOPPED AT 62%", theme::ACCENT),
        widgets::close_mark(false),
    ]
    .spacing(16)
    .align_y(Alignment::Center);

    column![
        super::paper_header(trailing.into(), theme::ACCENT),
        super::centered_body(container(center).into()),
        widgets::footer(),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
