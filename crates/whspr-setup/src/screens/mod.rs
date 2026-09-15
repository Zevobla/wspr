//! The installer's screens and the bits they share: the top-level `view`
//! dispatch, the paper header used by Installing/Done/Failure (a wordmark row
//! over a rule, with the whole band a window-drag handle), and the centered
//! body wrapper. The red-banded Install header is bespoke and lives in
//! `install`.

mod done;
mod failure;
mod install;
mod installing;

use iced::widget::{column, container, mouse_area, row, text, Space};
use iced::{Alignment, Background, Element, Length, Padding};

use crate::logo;
use crate::state::{Message, Screen, State};
use crate::theme;
use crate::widgets;

/// Renders the current screen on the paper ground.
pub fn view(state: &State) -> Element<'_, Message> {
    let content = match &state.screen {
        Screen::Install {
            expanded,
            autostart,
            start_menu,
            desktop,
        } => install::view(*expanded, *autostart, *start_menu, *desktop),
        Screen::Installing { progress } => installing::view(*progress),
        Screen::Done => done::view(),
        Screen::Failure { detail, at } => failure::view(detail.as_deref(), *at),
    };
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(theme::PAPER)),
            text_color: Some(theme::INK),
            ..container::Style::default()
        })
        .into()
}

/// The wordmark: the logo mark + "whspr". `on_red` selects the logo colorway
/// and the wordmark ink (paper on the red band, ink on paper).
pub fn wordmark<'a>(on_red: bool, size: f32) -> Element<'a, Message> {
    let ink = if on_red { theme::PAPER } else { theme::INK };
    row![
        logo::logo(on_red, size),
        text("whspr")
            .size(24.0)
            .font(theme::extrabold())
            .color(ink),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// The paper header for Installing/Done/Failure: the wordmark on the left,
/// `trailing` on the right, over a rule of `rule_color` (the neutral divider,
/// or the accent for Failure). The band above the rule is the window's drag
/// handle; `trailing`'s own buttons capture their presses first.
pub fn paper_header<'a>(
    trailing: Element<'a, Message>,
    rule_color: iced::Color,
) -> Element<'a, Message> {
    let bar = row![
        wordmark(false, 40.0),
        Space::new().width(Length::Fill),
        trailing,
    ]
    .align_y(Alignment::Center);
    column![
        mouse_area(
            container(bar)
                .width(Length::Fill)
                .padding(Padding {
                    top: 34.0,
                    right: 44.0,
                    bottom: 22.0,
                    left: 44.0,
                })
        )
        .on_press(Message::Drag),
        widgets::hrule(rule_color),
    ]
    .into()
}

/// A right-aligned uppercase caption in the paper header's trailing slot
/// (e.g. Done's "Installed", Failure's "Stopped at 62%"), colored `color`.
pub fn header_status<'a>(label: &'a str, color: iced::Color) -> Element<'a, Message> {
    text(label)
        .size(12.0)
        .font(theme::semibold())
        .color(color)
        .into()
}

/// Wraps a screen's center content in a full-height, centered body with the
/// standard 44px horizontal inset -- the shared frame for Installing / Done /
/// Failure between their header and the footer.
pub fn centered_body<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: 44.0,
            bottom: 0.0,
            left: 44.0,
        })
        .into()
}
