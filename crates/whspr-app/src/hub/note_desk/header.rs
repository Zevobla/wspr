//! The note desk's header band: the desk title on the left; a word count and a
//! ghost Back-to-Dictate button on the right, closed by the system 2px rule.
//! The desk shows a finished, imported note (no live capture yet), so it
//! carries no live meter / running timer / Stop control. Back emits
//! `Message::BackToDictate`.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::note_desk::NoteDeskState;
use crate::state::Message;
use crate::theme::styles::button as button_style;
use crate::theme::widgets;
use crate::theme::{color, spacing, type_scale};

/// The header band, `HEADER_H` tall, closed by the system 2px rule.
pub(super) fn view<'a>(
    nd: &'a NoteDeskState,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let left = row![text(nd.title.clone())
        .size(type_scale::TITLE_LARGE.size)
        .font(type_scale::TITLE_LARGE.font())
        .color(scheme.on_surface),]
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let right = row![
        text(format!("{} words", word_count(nd)))
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font())
            .color(scheme.on_surface_variant),
        back_button(scheme),
    ]
    .spacing(spacing::MD)
    .align_y(Alignment::Center);

    let bar = container(
        row![left, right]
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .height(Length::Fixed(spacing::layout::HEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding([0.0, spacing::LG]);

    column![bar, widgets::hr(scheme)].width(Length::Fill).into()
}

/// Total words across the transcript rows, for the header's `N words` readout.
fn word_count(nd: &NoteDeskState) -> usize {
    nd.rows
        .iter()
        .map(|r| r.text.split_whitespace().count())
        .sum()
}

/// The ghost Back-to-Dictate button (left-arrow), returning to the normal Hub.
fn back_button<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    button(
        row![
            text("\u{2190}")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
            text("Back to Dictate")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, status| button_style::text(scheme, status))
    .on_press(Message::BackToDictate)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_count_sums_row_words() {
        let nd = NoteDeskState::sample();
        assert!(word_count(&nd) > 0);
    }

    #[test]
    fn header_builds() {
        let _: Element<'_, Message> = view(&NoteDeskState::sample(), &color::LIGHT);
    }
}
