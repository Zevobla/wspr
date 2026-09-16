//! The note desk's Typst column: a 47px header (a page kicker / live export
//! status + the View code, Export .typ and Export PDF actions) over a
//! wash-backed preview with a centered white "page". The buttons are live --
//! View code toggles the raw `.typ` source, and the two exports open a save
//! dialog and write/compile off the UI thread (see `crate::note_desk`). The
//! page shows the note's real content (or its `.typ` source in code view).

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Color, Element, Length};

use crate::note_desk::{Gutter, NoteDeskState};
use crate::note_export::document_typ;
use crate::state::Message;
use crate::theme::styles::button as button_style;
use crate::theme::{color, spacing, styles, type_scale, widgets};

use super::{PAGE_W, SUBHEADER_H};

/// The page's ink -- fixed dark on the always-white page, so it stays legible
/// in the dark theme too (the comp keeps the page white in both).
const PAGE_INK: Color = Color::from_rgb8(0x20, 0x1e, 0x1d);

/// The Typst column: a header band, then the preview area.
pub(super) fn view<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    column![header(nd, scheme), widgets::hr(scheme), preview(nd, scheme)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The 47px header band: a page kicker (replaced by the live export status
/// when one is set) and the View code / Export .typ / Export PDF actions.
fn header<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let view_code_label = if nd.view_code {
        "View note"
    } else {
        "View code"
    };
    let actions = row![
        action_button(
            view_code_label,
            button_style::text,
            Message::NoteDeskToggleViewCode,
            scheme,
        ),
        action_button(
            "Export .typ",
            button_style::outlined,
            Message::NoteDeskExportTyp,
            scheme,
        ),
        action_button(
            "Export PDF",
            button_style::filled,
            Message::NoteDeskExportPdf,
            scheme,
        ),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    // The kicker doubles as the export status line: accent while a status is
    // set, dim page marker otherwise.
    let (kicker_label, kicker_color) = match &nd.export_status {
        Some(status) => (status.clone(), scheme.primary),
        None => ("PAGE 1 OF 1".to_string(), scheme.on_surface_variant),
    };

    container(
        row![
            text(kicker_label)
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(kicker_color)
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

/// A live header action: a styled button that emits `message` on press and
/// reacts to hover/press through its variant's own status styling.
fn action_button<'a>(
    label: &'a str,
    style: fn(&color::Scheme, button::Status) -> button::Style,
    message: Message,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .padding([4.0, 10.0])
    .on_press(message)
    .style(move |_theme, status| style(scheme, status))
    .into()
}

/// The preview area: a wash ground with a centered white page. The page shows
/// the note's rendered content, or its raw `.typ` source in code view.
fn preview<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let body = if nd.view_code {
        code_body(nd, scheme)
    } else {
        rendered_body(nd, scheme)
    };
    let page = container(body)
        .width(Length::Fixed(PAGE_W))
        .height(Length::Fill)
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

/// The rendered page: a running head, the note title, the kept chapters, and
/// every transcript row (kept rows carry an accent timestamp, mirroring both
/// the desk gutter and the exported `.typ`). Scrolls when it overflows.
fn rendered_body<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let dim = color::wash(PAGE_INK, 0.5);
    let accent = scheme.primary;

    let head = row![
        text(nd.title.clone())
            .size(8.0)
            .font(type_scale::KICKER.font())
            .color(dim)
            .width(Length::Fill),
        text("NOTES")
            .size(8.0)
            .font(type_scale::KICKER.font())
            .color(dim),
    ]
    .align_y(Alignment::Center);

    let head_rule = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme| container::Style {
            background: Some(Background::Color(PAGE_INK)),
            ..container::Style::default()
        });

    let title = text(nd.title.clone())
        .size(16.0)
        .font(type_scale::TITLE_MEDIUM.font())
        .color(PAGE_INK);

    let mut col = column![head, head_rule, title].spacing(spacing::SM);

    if !nd.headings.is_empty() {
        col = col.push(page_kicker("CHAPTERS", dim));
        for h in &nd.headings {
            col = col.push(
                text(format!("{}  ·  {}", h.time_label, h.title))
                    .size(10.0)
                    .font(type_scale::TITLE_MEDIUM.font())
                    .color(PAGE_INK),
            );
        }
    }

    col = col.push(page_kicker("TRANSCRIPT", dim));
    for r in &nd.rows {
        let time_color = if r.gutter == Gutter::Kept {
            accent
        } else {
            dim
        };
        col = col.push(
            row![
                text(r.time_label.clone())
                    .size(9.5)
                    .font(type_scale::LABEL_MEDIUM.font())
                    .color(time_color)
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

    scrollable(col.width(Length::Fill))
        .height(Length::Fill)
        .style(move |_theme, status| styles::scrollable::rail(scheme, status))
        .into()
}

/// The code view: the raw `document_typ` source in monospace. Scrolls when it
/// overflows.
fn code_body<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    scrollable(
        text(document_typ(nd))
            .size(9.0)
            .font(iced::Font::MONOSPACE)
            .line_height(iced::widget::text::LineHeight::Relative(1.4))
            .color(PAGE_INK)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .style(move |_theme, status| styles::scrollable::rail(scheme, status))
    .into()
}

/// A small uppercase section kicker on the white page.
fn page_kicker<'a>(label: &'a str, color: Color) -> Element<'a, Message> {
    text(label)
        .size(8.0)
        .font(type_scale::KICKER.font())
        .color(color)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typst_column_builds() {
        let _: Element<'_, Message> = view(&NoteDeskState::sample(), &color::LIGHT);
    }

    #[test]
    fn typst_column_renders_headings() {
        let nd = NoteDeskState::from_import(
            "Lecture",
            vec![crate::note_desk::NoteHeading {
                time_label: "00:00".to_string(),
                title: "Intro".to_string(),
            }],
            &whspr_core::Transcript::default(),
        );
        let _: Element<'_, Message> = view(&nd, &color::LIGHT);
    }

    #[test]
    fn typst_column_builds_in_code_view() {
        let mut nd = NoteDeskState::sample();
        nd.view_code = true;
        let _: Element<'_, Message> = view(&nd, &color::LIGHT);
    }

    #[test]
    fn typst_column_shows_the_export_status() {
        let mut nd = NoteDeskState::sample();
        nd.export_status = Some("Saved PDF to /tmp/x.pdf".to_string());
        let _: Element<'_, Message> = view(&nd, &color::LIGHT);
    }
}
