//! The note desk's header band: the desk title + a live meter on the left,
//! the elapsed timer, a primary Stop button (square icon, a visual stub this
//! phase) and a ghost Back-to-Dictate button on the right, closed by the
//! system 2px rule. Stop's finalize handler lands in a later phase; Back
//! emits `Message::BackToDictate`.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::note_desk::NoteDeskState;
use crate::state::Message;
use crate::theme::styles::button as button_style;
use crate::theme::widgets::{self, meter};
use crate::theme::{color, icons, spacing, type_scale};

/// The header band, `HEADER_H` tall, closed by the system 2px rule.
pub(super) fn view<'a>(nd: &'a NoteDeskState, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let left = row![
        text(nd.title.clone())
            .size(type_scale::TITLE_LARGE.size)
            .font(type_scale::TITLE_LARGE.font())
            .color(scheme.on_surface),
        // Static level this phase -- no live audio wired into the desk yet.
        meter(0.55, 20, 28.0, scheme),
    ]
    .spacing(spacing::LG)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let right = row![
        text(elapsed_label(nd.timer_start.elapsed()))
            .size(type_scale::TITLE_MEDIUM.size)
            .font(type_scale::TITLE_MEDIUM.font())
            .color(scheme.on_surface),
        stop_button(scheme),
        back_button(scheme),
    ]
    .spacing(spacing::SM)
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

/// The primary Stop button -- a visual stub this phase (the finalize handler
/// lands later), so it carries no `on_press` but keeps its active look.
fn stop_button<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    button(
        row![
            icons::icon(icons::SQUARE, 12.0, scheme.on_primary),
            text("Stop")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .padding([spacing::SM, spacing::MD])
    .style(move |_theme, _status| button_style::filled(scheme, button::Status::Active))
    .into()
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

/// Formats an elapsed duration as `MM:SS` (tabular).
fn elapsed_label(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_label_is_mm_ss() {
        assert_eq!(elapsed_label(std::time::Duration::from_secs(0)), "00:00");
        assert_eq!(elapsed_label(std::time::Duration::from_secs(754)), "12:34");
    }

    #[test]
    fn header_builds() {
        let _: Element<'_, Message> = view(&NoteDeskState::sample(), &color::LIGHT);
    }
}
