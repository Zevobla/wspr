//! Screen 2 -- the progress screen. A paper header over a vertically-centered
//! block: the "Setting up…" title, an 8px progress bar (neutral-300 track,
//! accent fill), and a step/percent row. The bar and step are driven by the
//! simulated install's `progress` (see `crate::begin_install`); the step
//! label is derived from `progress` so the two never drift.

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Background, Element, Length};

use crate::state::{Message, Step};
use crate::theme;
use crate::widgets;

pub fn view(progress: u8) -> Element<'static, Message> {
    let center = column![
        text("Setting up\u{2026}")
            .size(56.0)
            .font(theme::extrabold())
            .color(theme::INK),
        Space::new().height(Length::Fixed(30.0)),
        progress_bar(progress),
        Space::new().height(Length::Fixed(16.0)),
        row![
            text(Step::at(progress).label())
                .size(12.0)
                .font(theme::semibold())
                .color(theme::INK),
            Space::new().width(Length::Fill),
            text(format!("{progress}%"))
                .size(12.0)
                .font(theme::semibold())
                .color(theme::INK),
        ]
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill);

    column![
        super::paper_header(Space::new().into(), theme::DIVIDER),
        super::centered_body(center.into()),
        widgets::footer(),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The 8px progress bar: a neutral-300 track with an accent fill `progress`
/// percent wide, drawn as two flex-weighted segments so the split is exact.
fn progress_bar(progress: u8) -> Element<'static, Message> {
    let fill = container(Space::new())
        .width(Length::FillPortion(progress as u16))
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(theme::ACCENT)),
            ..container::Style::default()
        });
    let rest = container(Space::new())
        .width(Length::FillPortion(100u16.saturating_sub(progress as u16)))
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(theme::NEUTRAL_300)),
            ..container::Style::default()
        });
    container(row![fill, rest])
        .width(Length::Fill)
        .height(Length::Fixed(8.0))
        .into()
}
