//! Screen 3 -- the success screen. A paper header ("Installed" + close mark)
//! over a centered block: the "whspr is ready." title, the Ctrl/Shift/Space
//! key chips, the one-line how-to, a dimmed first-run note, and the primary
//! "Open whspr" button.

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::state::Message;
use crate::theme;
use crate::widgets;

pub fn view() -> Element<'static, Message> {
    let chips = row![
        widgets::key_chip("Ctrl", false),
        widgets::key_chip("Shift", false),
        widgets::key_chip("Space", true),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    let center = column![
        text("whspr is ready.")
            .size(52.0)
            .font(theme::extrabold())
            .color(theme::INK),
        Space::new().height(Length::Fixed(28.0)),
        chips,
        Space::new().height(Length::Fixed(28.0)),
        text("Hold it and speak; your words type themselves in, anywhere.")
            .size(18.0)
            .font(theme::regular())
            .color(theme::INK),
        Space::new().height(Length::Fixed(12.0)),
        text(
            "whspr lives in your system tray. On first run it\u{2019}ll ask for microphone \
             access and let you pick a speech model."
        )
        .size(14.0)
        .font(theme::regular())
        .color(theme::DIMMED),
        Space::new().height(Length::Fixed(28.0)),
        widgets::primary_button("Open whspr", Message::OpenApp),
    ]
    .width(Length::Fill);

    let trailing = row![
        super::header_status("INSTALLED", theme::ACCENT),
        widgets::close_mark(false),
    ]
    .spacing(16)
    .align_y(Alignment::Center);

    column![
        super::paper_header(trailing.into(), theme::DIVIDER),
        super::centered_body(container(center).into()),
        widgets::footer(),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
