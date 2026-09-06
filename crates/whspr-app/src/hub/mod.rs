//! The Hub window: settings, device list, hotkey config, history, and
//! stats -- restyled on the MD3 tokens in `crate::theme` (see that
//! module's doc comment for the overall token system this app styles
//! from). Every section is an M3 "card" (`section`, tonal
//! `surface_container_low`); every field caption/control pair is grouped
//! with `field`.

use iced::widget::{button, column, container, progress_bar, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

mod common;
mod settings;
mod speakers;

use crate::state::{Message, State};
use crate::stats;
use crate::theme::{self, color, spacing, styles, type_scale};
use common::section;

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    let scheme = theme::scheme(&state.theme);

    let body = scrollable(
        column![
            settings::view(state, scheme),
            transcribe_section(state, scheme),
            speakers::view(state, scheme),
            history_section(state, scheme),
            stats_section(state, scheme),
        ]
        .spacing(spacing::XL),
    )
    .style(move |_theme, status| styles::scrollable::rail(scheme, status));

    container(
        column![
            header(state, scheme),
            divider(scheme),
            error_banner(state, scheme),
            body
        ]
        .spacing(spacing::LG)
        .padding(spacing::XL)
        .width(Length::Fill),
    )
    .style(move |_theme| styles::container::surface(scheme))
    .into()
}

/// The theme-toggle button's label names the theme you'd switch *to*, not
/// the one you're currently in -- kept as a pure `&Theme -> &str` so the
/// wording stays testable without standing up the whole `header` view.
fn theme_toggle_label(theme: &iced::Theme) -> &'static str {
    match theme {
        iced::Theme::Dark => "Switch to light",
        _ => "Switch to dark",
    }
}

fn header<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let theme_label = theme_toggle_label(&state.theme);

    row![
        text("whspr")
            .size(type_scale::TITLE_LARGE.size)
            .font(type_scale::TITLE_LARGE.font())
            .color(scheme.on_surface)
            .width(Length::Fill),
        button(
            text(theme_label)
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font())
        )
        .style(move |_theme, status| styles::button::text(scheme, status))
        .on_press(Message::ThemeToggled),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A 1px `outline_variant` rule under the header.
fn divider(scheme: &'static color::Scheme) -> Element<'static, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme| styles::container::divider(scheme))
        .into()
}

fn error_banner<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match &state.last_error {
        Some(error) => container(
            text(format!("Last worker error: {error}"))
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font()),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| styles::container::error_banner(scheme))
        .into(),
        None => column![].into(),
    }
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

fn transcribe_section<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let pick_button = button(
        text("Transcribe a file...")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::filled(scheme, status))
    .on_press(Message::PickFileToTranscribe);

    let recording = state.is_recording;
    let record_button = button(
        text(if recording {
            "Stop recording"
        } else {
            "Record"
        })
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| {
        if recording {
            styles::button::filled(scheme, status)
        } else {
            styles::button::tonal(scheme, status)
        }
    })
    .on_press(Message::ToggleRecording);

    let controls = row![pick_button, record_button].spacing(spacing::MD);

    // Live input-level meter while recording (RMS scaled up for visibility).
    let meter: Element<'_, Message> = if recording {
        progress_bar(0.0..=1.0, (state.mic_level * 4.0).min(1.0))
            .style(move |_theme| styles::progress_bar::thinking(scheme))
            .into()
    } else {
        column![].into()
    };

    let status: Element<'_, Message> = match &state.transcribe_status {
        Some(status) => text(status.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => column![].into(),
    };

    let recognized: Element<'_, Message> = match &state.transcribed_text {
        Some(t) if !t.trim().is_empty() => scrollable(
            text(t.clone())
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface),
        )
        .height(Length::Fixed(120.0))
        .style(move |_theme, status| styles::scrollable::rail(scheme, status))
        .into(),
        Some(_) => text("(empty transcript)")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => text("Record from the mic or pick a .wav file to transcribe it on-screen.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    section(
        scheme,
        "Transcribe & record",
        column![controls, meter, status, recognized]
            .spacing(spacing::MD)
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_toggle_label_names_the_target_theme() {
        // The label advertises the theme the click switches *to*.
        assert_eq!(theme_toggle_label(&iced::Theme::Dark), "Switch to light");
        assert_eq!(theme_toggle_label(&iced::Theme::Light), "Switch to dark");
    }
}
