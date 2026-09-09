//! The note desk's Typst column: a 47px header ("Page 1 of 1" + inert View
//! code / Export .typ / Export PDF stubs) over a wash-backed preview with a
//! centered white "page". A placeholder this phase -- real Typst rendering
//! (the compiled page) lands in a later phase, so the buttons carry no
//! handlers yet and the page shows a couple of sample lines.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Background, Color, Element, Length};

use crate::note_desk::NoteDeskState;
use crate::state::Message;
use crate::theme::styles::button as button_style;
use crate::theme::widgets;
use crate::theme::{color, spacing, type_scale};

use super::{PAGE_W, SUBHEADER_H};

/// The page's ink -- fixed dark on the always-white page, so it stays legible
/// in the dark theme too (the comp keeps the page white in both).
const PAGE_INK: Color = Color::from_rgb8(0x20, 0x1e, 0x1d);

/// The Typst column: a header band, then the preview area.
pub(super) fn view<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    column![header(scheme), widgets::hr(scheme), preview(nd, scheme)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The 47px header band: a page kicker and the export/view actions (all inert
/// stubs this phase).
fn header<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    let actions = row![
        inert_button("View code", button_style::text, scheme),
        inert_button("Export .typ", button_style::outlined, scheme),
        inert_button("Export PDF", button_style::filled, scheme),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    container(
        row![
            text("PAGE 1 OF 1")
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(scheme.on_surface_variant)
                .width(Length::Fill),
            actions,
        ]
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(SUBHEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding([0.0, spacing::LG])
    .into()
}

/// A styled-but-inert button (no handler this phase). Its style closure
/// ignores the interaction status and always renders the active look, so the
/// stub reads like the comp's live control rather than a disabled one.
fn inert_button<'a>(
    label: &'a str,
    style: fn(&color::Scheme, button::Status) -> button::Style,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([4.0, 10.0])
    .style(move |_theme, _status| style(scheme, button::Status::Active))
    .into()
}

/// The preview area: a wash ground with a centered white page placeholder.
fn preview<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let page = container(page_body(nd))
        .width(Length::Fixed(PAGE_W))
        .padding(iced::Padding {
            top: spacing::XL,
            right: spacing::XL,
            bottom: 0.0,
            left: spacing::XL,
        })
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::WHITE)),
            ..container::Style::default()
        });

    container(page)
        .center_x(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: spacing::LG,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        })
        .style(move |_theme| container::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.09))),
            ..container::Style::default()
        })
        .into()
}

/// The white page's contents: a running head, the section title, and a couple
/// of sample lines drawn from the transcript rows.
fn page_body<'a>(nd: &'a NoteDeskState) -> Element<'a, Message> {
    let dim = color::wash(PAGE_INK, 0.5);

    let head = row![
        text(nd.title.clone())
            .size(8.0)
            .font(type_scale::KICKER.font())
            .color(dim)
            .width(Length::Fill),
        text("9 Sep 2026")
            .size(8.0)
            .font(type_scale::KICKER.font())
            .color(dim),
    ]
    .align_y(Alignment::Center);

    let title = text("2 · Microstates")
        .size(14.0)
        .font(type_scale::TITLE_MEDIUM.font())
        .color(PAGE_INK);

    let head_rule = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme| container::Style {
            background: Some(Background::Color(PAGE_INK)),
            ..container::Style::default()
        });

    let mut col = column![head, head_rule, title].spacing(spacing::SM);
    for r in nd.rows.iter().take(4) {
        col = col.push(
            row![
                text(r.time_label.clone())
                    .size(9.5)
                    .font(type_scale::LABEL_MEDIUM.font())
                    .color(dim)
                    .width(Length::Fixed(34.0)),
                text(r.text.clone())
                    .size(9.5)
                    .font(type_scale::BODY_MEDIUM.font())
                    .line_height(iced::widget::text::LineHeight::Relative(1.5))
                    .color(PAGE_INK)
                    .width(Length::Fill),
            ]
            .spacing(spacing::SM),
        );
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typst_column_builds() {
        let _: Element<'_, Message> = view(&NoteDeskState::sample(), &color::LIGHT);
    }
}
