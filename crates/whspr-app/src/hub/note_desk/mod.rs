//! The full-screen longform "note desk": a collapsed nav rail, a header band,
//! and a two-column transcript/Typst body that replaces the normal Hub
//! nav-rail + header shell wholesale (see `crate::hub::view`, which
//! short-circuits into `view` here when `State::note_desk` is set). Entered
//! and left manually for now (`Message::EnterNoteDesk` / `BackToDictate`);
//! auto-morph, live transcription and real Typst rendering are later phases.
//! Split into `header`, `transcript` and `notes_pane` submodules so no file
//! nears the 600-line cap (AA-06).

mod header;
mod notes_pane;
mod transcript;

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::note_desk::NoteDeskState;
use crate::state::{Message, State};
use crate::theme::widgets::{self, Mark};
use crate::theme::{color, spacing, styles, type_scale};

/// Height of a body column's header row (the "TRANSCRIPT" / "Page 1 of 1"
/// bands), matched to the comp.
pub(super) const SUBHEADER_H: f32 = 47.0;
/// The transcript column's fixed width; the Typst column takes the rest.
pub(super) const TRANSCRIPT_W: f32 = 452.0;
/// The Typst preview's white "page" width.
pub(super) const PAGE_W: f32 = 396.0;

/// A rail number item's height.
const RAIL_ITEM_H: f32 = 38.0;

/// Renders the note desk. `_state` is threaded through to match the Hub's
/// `view` dispatch shape (the theme it drives is already resolved into
/// `scheme`); `nd` is the desk's own state.
pub fn view<'a>(
    _state: &'a State,
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let body = row![
        transcript::view(nd, scheme),
        widgets::vrule(spacing::layout::RULE, scheme),
        notes_pane::view(nd, scheme),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let main = column![header::view(nd, scheme), body]
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        row![
            rail(scheme),
            widgets::vrule(spacing::layout::RULE, scheme),
            main,
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme| styles::container::surface(scheme))
    .into()
}

/// The collapsed nav rail: a brand square, numbered items 01-05 (01 active),
/// and a bottom mic-status block.
fn rail<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        column![
            rail_brand(scheme),
            widgets::hr(scheme),
            rail_items(scheme),
            Space::new().height(Length::Fill),
            widgets::hr(scheme),
            rail_status(scheme),
        ]
        .width(Length::Fill)
        .align_x(Alignment::Center),
    )
    .width(Length::Fixed(spacing::layout::RAIL_W_COLLAPSED))
    .height(Length::Fill)
    .style(move |_theme| styles::container::rail(scheme))
    .into()
}

/// The rail's brand block: a 12px accent square bottom-aligned in a
/// `RAIL_HEADER_H` band, matched to the desk header's height.
fn rail_brand<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(widgets::status_square(Mark::Solid, 12.0, scheme))
        .width(Length::Fill)
        .height(Length::Fixed(spacing::layout::RAIL_HEADER_H))
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding([0.0, 0.0, spacing::LG, 0.0])
        .into()
}

/// The numbered rail items 01-05, with 01 active.
fn rail_items<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut col = column![].width(Length::Fill);
    for n in 1..=5u8 {
        col = col.push(rail_number(n, n == 1, scheme));
    }
    container(col)
        .padding([spacing::MD, 0.0])
        .width(Length::Fill)
        .into()
}

/// One rail number: accent + an inset right rule when active, dim otherwise.
fn rail_number<'a>(n: u8, active: bool, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let ink = if active {
        scheme.primary
    } else {
        color::wash(scheme.on_surface, 0.45)
    };
    let num = container(
        text(format!("{n:02}"))
            .size(type_scale::KICKER.size)
            .font(type_scale::KICKER.font())
            .color(ink),
    )
    .width(Length::Fill)
    .height(Length::Fixed(RAIL_ITEM_H))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center);

    if active {
        let rule = container(Space::new())
            .width(Length::Fixed(3.0))
            .height(Length::Fixed(RAIL_ITEM_H))
            .style(move |_theme| styles::container::accent(scheme));
        row![num, rule].width(Length::Fill).into()
    } else {
        num.into()
    }
}

/// The rail's bottom status: an accent dot over a vertical mic label.
fn rail_status<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        column![
            widgets::status_square(Mark::Solid, 10.0, scheme),
            vertical_label("MACBOOK PRO MIC", scheme),
        ]
        .spacing(spacing::SM)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .padding([spacing::LG, 0.0])
    .into()
}

/// A vertical (stacked-glyph) label -- iced has no text rotation, so the
/// characters stack instead. Spaces become a small gap.
fn vertical_label<'a>(label: &str, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut col = column![].align_x(Alignment::Center).spacing(1.0);
    for ch in label.chars() {
        if ch == ' ' {
            col = col.push(Space::new().height(Length::Fixed(4.0)));
        } else {
            col = col.push(
                text(ch.to_string())
                    .size(9.0)
                    .font(type_scale::KICKER.font())
                    .color(color::wash(scheme.on_surface, 0.45)),
            );
        }
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_config::Config;

    #[test]
    fn note_desk_view_builds() {
        let state = State::new(Config::default());
        let nd = NoteDeskState::sample();
        let _: Element<'_, Message> = view(&state, &nd, &color::LIGHT);
    }
}
