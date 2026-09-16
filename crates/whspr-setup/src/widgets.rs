//! The installer's shared Modernist widgets, built from iced primitives the
//! same way `whspr-app`'s `theme::widgets` are -- primary/ghost buttons, the
//! close mark, the footer strip, 2px/1px rules, the custom option toggle and
//! its row, and the Done screen's key chips. Every corner is square and
//! every rule is a flat container, never elevation.

use iced::widget::svg::{Handle, Svg};
use iced::widget::{button, column, container, mouse_area, row, stack, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::state::Message;
use crate::theme;

/// A 2px horizontal rule in `color` -- the system's strong section divider.
pub fn hrule<'a>(color: Color) -> Element<'a, Message> {
    rule(color, 2.0)
}

/// A 1px hairline rule in `color` -- between option rows.
pub fn hairline<'a>(color: Color) -> Element<'a, Message> {
    rule(color, 1.0)
}

fn rule<'a>(color: Color, height: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}

/// The primary CTA: an accent fill with centered paper label, fixed 200px
/// wide (the spec's min-width) and zero radius.
pub fn primary_button<'a>(label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(
        container(
            text(label)
                .size(15.0)
                .font(theme::semibold())
                .color(theme::PAPER),
        )
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
    .width(Length::Fixed(200.0))
    .padding([13.0, 20.0])
    .on_press(on_press)
    .style(theme::primary)
    .into()
}

/// The ghost button: a 2px ink border, transparent, ink label. Sized to its
/// content (used for "Options ▾/▴" next to the primary CTA).
pub fn ghost_button<'a>(label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(
        text(label)
            .size(15.0)
            .font(theme::semibold())
            .color(theme::INK),
    )
    .padding([13.0, 20.0])
    .on_press(on_press)
    .style(theme::ghost)
    .into()
}

/// The lone close mark: an X of two 2px strokes in a 14x14 box, squared caps,
/// paper on the red header / ink on paper. A little padding gives it a
/// comfortable hit target without enlarging the glyph.
pub fn close_mark<'a>(on_red: bool) -> Element<'a, Message> {
    let stroke = if on_red { theme::PAPER } else { theme::INK };
    let markup = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14"><line x1="1" y1="1" x2="13" y2="13" stroke="{hex}" stroke-width="2" stroke-linecap="square"/><line x1="13" y1="1" x2="1" y2="13" stroke="{hex}" stroke-width="2" stroke-linecap="square"/></svg>"##,
        hex = hex(stroke)
    );
    let glyph = Svg::new(Handle::from_memory(markup.into_bytes()))
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(14.0));
    button(glyph)
        .padding(4)
        .on_press(Message::Close)
        .style(|_theme, _status| button::Style {
            background: None,
            ..button::Style::default()
        })
        .into()
}

/// The footer strip pinned to the window bottom: a 2px divider then a
/// space-between row of the four facts, all uppercase 11px SemiBold. Only
/// `whspr.exe` is ink; the rest are dimmed.
pub fn footer<'a>() -> Element<'a, Message> {
    let fact =
        |label: &'a str, color: Color| text(label).size(11.0).font(theme::semibold()).color(color);
    column![
        hrule(theme::DIVIDER),
        container(
            row![
                fact("WHSPR.EXE", theme::INK),
                Space::new().width(Length::Fill),
                fact("WINDOWS 10/11", theme::DIMMED),
                Space::new().width(Length::Fill),
                fact("X64 & ARM64", theme::DIMMED),
                Space::new().width(Length::Fill),
                fact("BUILD 1.0 (204)", theme::DIMMED),
            ]
            .align_y(Alignment::Center)
        )
        .padding([13.0, 44.0]),
    ]
    .into()
}

/// The custom toggle switch's non-interactive visual: a 52x28 box with a 2px
/// ink border and an 18x18 knob. ON = accent track, paper knob, knob right;
/// OFF = transparent track, ink knob, knob left. The click is owned by the
/// enclosing row (see [`option_row`]).
fn toggle_visual<'a>(is_on: bool) -> Element<'a, Message> {
    let knob = container(Space::new())
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(if is_on {
                theme::PAPER
            } else {
                theme::INK
            })),
            ..container::Style::default()
        });
    let inner = if is_on {
        row![Space::new().width(Length::Fill), knob]
    } else {
        row![knob, Space::new().width(Length::Fill)]
    }
    .align_y(Alignment::Center);
    container(inner)
        .width(Length::Fixed(52.0))
        .height(Length::Fixed(28.0))
        .padding(3)
        .style(move |_theme| container::Style {
            background: is_on.then_some(Background::Color(theme::ACCENT)),
            border: Border {
                color: theme::INK,
                width: 2.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One option row: a title + subtitle on the left, the toggle and its
/// "On"/"Off" state on the right, the whole row clickable to flip it.
pub fn option_row<'a>(
    title: &'a str,
    subtitle: &'a str,
    is_on: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let labels = column![
        text(title)
            .size(15.0)
            .font(theme::semibold())
            .color(theme::INK),
        text(subtitle)
            .size(12.0)
            .font(theme::regular())
            .color(theme::DIMMED),
    ]
    .spacing(3);
    let state = text(if is_on { "ON" } else { "OFF" })
        .size(11.0)
        .font(theme::semibold())
        .color(theme::DIMMED);
    let trailing = row![toggle_visual(is_on), state]
        .spacing(12)
        .align_y(Alignment::Center);
    let content =
        row![labels, Space::new().width(Length::Fill), trailing].align_y(Alignment::Center);
    mouse_area(container(content).padding([14.0, 0.0]))
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(on_press)
        .into()
}

/// A Done-screen key chip: a squared paper cap with a 2px ink border and a
/// hard `3px 3px 0` ink offset shadow (a second ink box behind, shifted down
/// and right via a `stack`). `wide` widens the "Space" cap.
pub fn key_chip<'a>(label: &'a str, wide: bool) -> Element<'a, Message> {
    let w = if wide { 104.0 } else { 60.0 };
    let h = 48.0;
    let shadow = container(Space::new())
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .style(|_theme| container::Style {
            background: Some(Background::Color(theme::INK)),
            ..container::Style::default()
        });
    let face = container(
        text(label)
            .size(18.0)
            .font(theme::semibold())
            .color(theme::INK),
    )
    .width(Length::Fixed(w))
    .height(Length::Fixed(h))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(|_theme| container::Style {
        background: Some(Background::Color(theme::PAPER)),
        border: Border {
            color: theme::INK,
            width: 2.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });
    stack![
        container(shadow).padding(Padding {
            top: 3.0,
            left: 3.0,
            bottom: 0.0,
            right: 0.0,
        }),
        face,
    ]
    .into()
}

/// Formats a `Color`'s RGB as a `#rrggbb` hex string for inline SVG markup.
fn hex(color: Color) -> String {
    let ch = |c: f32| (c * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ch(color.r), ch(color.g), ch(color.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_formats_the_modernist_tokens() {
        assert_eq!(hex(theme::ACCENT), "#ec3013");
        assert_eq!(hex(theme::INK), "#201e1d");
        assert_eq!(hex(theme::PAPER), "#f3f2f2");
    }
}
