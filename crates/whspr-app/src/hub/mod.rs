//! The Hub window: a header, a top tab bar, and one screen at a time below
//! it -- Dictate (the default), Speakers, History, Settings -- restyled on
//! the MD3 tokens in `crate::theme` (see that module's doc comment for the
//! overall token system this app styles from). Every screen module
//! (`dictate`, `speakers`, `history`, `settings`) is an M3 "card" list
//! (`common::section`, tonal `surface_container_low`); every field
//! caption/control pair is grouped with `common::field`.
//!
//! whspr's core action -- record/dictate -- is Dictate, not buried among
//! settings as an equally-weighted card the way the old single-column
//! layout had it; see `crate::state::Screen` for the enum driving which
//! screen is showing.

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

mod common;
mod dictate;
mod history;
mod settings;
mod speakers;

use crate::state::{Message, Screen, State};
use crate::theme::{self, color, spacing, styles, type_scale};

/// Every tab, in the order the tab bar shows them.
const SCREENS: [Screen; 4] = [
    Screen::Dictate,
    Screen::Speakers,
    Screen::History,
    Screen::Settings,
];

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    let scheme = theme::scheme(&state.theme);

    let screen_content = match state.screen {
        Screen::Dictate => dictate::view(state, scheme),
        Screen::Speakers => speakers::view(state, scheme),
        Screen::History => history::view(state, scheme),
        Screen::Settings => settings::view(state, scheme),
    };

    let body = scrollable(screen_content)
        .width(Length::Fill)
        .style(move |_theme, status| styles::scrollable::rail(scheme, status));

    container(
        column![
            header(state, scheme),
            tab_bar(state.screen, scheme),
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

/// The tab bar's button label for a screen -- pure so the wording is
/// testable without standing up the whole tab bar (mirrors
/// `theme_toggle_label`'s reasoning below).
fn tab_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Dictate => "Dictate",
        Screen::Speakers => "Speakers",
        Screen::History => "History",
        Screen::Settings => "Settings",
    }
}

/// The top tab bar: one button per `Screen`, the active one styled `tonal`
/// (a secondary-container tint) and every other one styled `text` (no
/// background) -- the same active/inactive mapping MD3 uses for segmented
/// controls, reusing the button styles `crate::hub::settings`'s hotkey
/// preview and this Hub's own header already style from.
fn tab_bar(current: Screen, scheme: &'static color::Scheme) -> Element<'static, Message> {
    row(SCREENS.map(|screen| tab_button(scheme, screen, current == screen)))
        .spacing(spacing::SM)
        .into()
}

fn tab_button(
    scheme: &'static color::Scheme,
    screen: Screen,
    active: bool,
) -> Element<'static, Message> {
    button(
        text(tab_label(screen))
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| {
        if active {
            styles::button::tonal(scheme, status)
        } else {
            styles::button::text(scheme, status)
        }
    })
    .on_press(Message::TabSelected(screen))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_toggle_label_names_the_target_theme() {
        // The label advertises the theme the click switches *to*.
        assert_eq!(theme_toggle_label(&iced::Theme::Dark), "Switch to light");
        assert_eq!(theme_toggle_label(&iced::Theme::Light), "Switch to dark");
    }

    #[test]
    fn every_tab_has_a_distinct_label() {
        let labels: Vec<&str> = SCREENS.iter().map(|&s| tab_label(s)).collect();
        // No two tabs share a label, so the bar is unambiguous.
        for (i, a) in labels.iter().enumerate() {
            for b in &labels[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(tab_label(Screen::Dictate), "Dictate");
    }
}
