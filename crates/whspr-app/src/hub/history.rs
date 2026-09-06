//! The History screen: completed transcriptions (`history_section`)
//! alongside this session's stats (`stats_section`).

use iced::widget::{column, scrollable, text};
use iced::{Element, Length};

use crate::state::{Message, State};
use crate::stats;
use crate::theme::{color, spacing, styles, type_scale};

use super::common::section;

/// Renders the History screen: past transcriptions plus this session's
/// stats, stacked in the same order they used to appear as separate Hub
/// cards.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        history_section(state, scheme),
        stats_section(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}

fn history_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let content: Element<'_, Message> = if state.history.is_empty() {
        text("No transcriptions yet -- dictate something to see it here.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into()
    } else {
        // Most recent first.
        let entries: Vec<Element<'_, Message>> = state
            .history
            .iter()
            .rev()
            .map(|entry| {
                text(entry.text.clone())
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface)
                    .into()
            })
            .collect();

        scrollable(column(entries).spacing(spacing::SM))
            .height(Length::Fixed(160.0))
            .style(move |_theme, status| styles::scrollable::rail(scheme, status))
            .into()
    };

    section(scheme, "History", content)
}

fn stats_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let line = |content: String| {
        text(content)
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface)
    };

    let count_line = line(format!(
        "Transcriptions this session: {}",
        state.history.len()
    ));

    let wpm_line = match stats::average_wpm(&state.history) {
        Some(wpm) => line(format!("Average speed: {wpm:.0} wpm")),
        None => line("Average speed: not enough data yet".to_string()),
    };

    section(
        scheme,
        "Stats",
        column![count_line, wpm_line].spacing(spacing::XS).into(),
    )
}
