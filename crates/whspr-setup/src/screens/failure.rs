//! The Failure screen -- a step couldn't complete. A paper header with a
//! "Stopped at N%" status and a 2px **accent** rule (not the neutral divider)
//! over a centered block: the "Couldn't finish." title, a reassuring line, the
//! error detail, and the primary "Retry" button.
//!
//! Reachable via `WHSPR_SETUP_SCREEN=failure` (which renders the canned design)
//! and from a genuine install error (`detail = Some(..)`, see
//! `crate::begin_install` / `crate::install`), which shows the real message and
//! the bar value it stopped at.

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::state::Message;
use crate::theme;
use crate::widgets;

/// The canned error line shown when there's no real error to report (the
/// `WHSPR_SETUP_SCREEN=failure` design-validation path).
const CANNED_ERROR: &str = "ERROR 0X80070005 \u{00B7} COPYING FILES";

/// Longest real error rendered before it's clipped, so a long OS message (with
/// a full path) can't overflow the fixed 900x620 window.
const MAX_DETAIL: usize = 160;

pub fn view(detail: Option<&str>, at: u8) -> Element<'static, Message> {
    let error_line = detail
        .map(|d| clip(d, MAX_DETAIL))
        .unwrap_or_else(|| CANNED_ERROR.to_string());

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
        text(error_line)
            .size(12.0)
            .font(theme::semibold())
            .color(theme::DIMMED),
        Space::new().height(Length::Fixed(28.0)),
        widgets::primary_button("Retry", Message::Retry),
    ]
    .width(Length::Fill);

    // The header status mirrors `super::header_status` but owns a formatted
    // String (so the whole view stays `'static`).
    let status = text(format!("STOPPED AT {at}%"))
        .size(12.0)
        .font(theme::semibold())
        .color(theme::ACCENT);

    let trailing = row![status, widgets::close_mark(false)]
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

/// Clips `s` to at most `max` chars (on a char boundary), appending an
/// ellipsis when it had to cut.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}\u{2026}")
    }
}
