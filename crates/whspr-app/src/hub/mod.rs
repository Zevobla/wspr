//! The Hub window: a left numbered nav rail, and one screen at a time to
//! its right (a header band over a scrolling body). Restyled on the
//! Modernist tokens in `crate::theme` -- flat, architectural, set in
//! Archivo, near-mono red on paper, zero radius, strong 2px rules (see that
//! module's doc comment).
//!
//! The rail (01 Dictate / 02 History / 03 Models / 04 Speakers / 05
//! Settings) replaces the old top tab bar; `crate::state::Screen`'s
//! declaration order is the rail order. Each screen module (`dictate`,
//! `history`, `models`, `speakers`, `settings`) renders its own body from
//! the shared widgets in `crate::theme::widgets`.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Element, Length};

// Windows-only custom caption/resize chrome for the borderless window (see
// `window_settings`). Compiled only on Windows so macOS/Linux chrome is
// untouched.
#[cfg(target_os = "windows")]
mod caption_windows;
mod common;
mod dictate;
mod history;
mod link_import;
mod models;
pub mod note_desk;
pub(crate) mod settings;
mod speakers;

use crate::state::{Message, Screen, State};
use crate::theme::widgets::{self, screen_header};
use crate::theme::{color, spacing, styles, type_scale};

/// Every screen, in nav-rail order.
const SCREENS: [Screen; 5] = [
    Screen::Dictate,
    Screen::History,
    Screen::Models,
    Screen::Speakers,
    Screen::Settings,
];

/// Archivo's descent ratio -- the fraction of a font's size that sits
/// *below* the glyph baseline (descender / units-per-em, as iced lays the
/// line out). Used to turn a font-size difference into a baseline offset.
/// ~0.3 for Archivo; tunable after a real-window check.
const ARCHIVO_DESCENT_RATIO: f32 = 0.3;

/// How much higher the brand's smaller wordmark must ride so its glyph
/// baseline lands on the screen title's. Both bands bottom-align their
/// content with `HEADER_TITLE_PAD_BOTTOM`, which aligns the text *boxes*,
/// not the baselines: the 28px `TITLE_LARGE` screen title has a larger
/// descent than the 18px `TITLE_MEDIUM` brand, so its baseline sits higher
/// and the brand hangs low. The gap is
/// `descent_ratio × (TITLE_LARGE − TITLE_MEDIUM)` ≈ 0.3 × (28 − 18) ≈ 3px,
/// which we add to the brand's bottom padding to lift it onto the shared
/// baseline. Tunable after a real-window check.
const HEADER_BASELINE_COMPENSATION: f32 =
    (type_scale::TITLE_LARGE.size - type_scale::TITLE_MEDIUM.size) * ARCHIVO_DESCENT_RATIO;

/// The brand block's padding. Left is 20px -- flush with the "01/02/..."
/// nav numbers below it -- the same on every platform. Vertically the brand
/// bottom-aligns in its `RAIL_HEADER_H` band just as the screen header's
/// title does in `HEADER_H` (see `brand` below and
/// `crate::theme::widgets::screen_header`), but with an extra
/// `HEADER_BASELINE_COMPENSATION` on top of `HEADER_TITLE_PAD_BOTTOM`:
/// bottom-alignment aligns the text *boxes*, and the 18px "whspr" has a
/// smaller descent than the 28px screen title, so without the lift its
/// baseline would hang ~3px below the title's. The compensation raises
/// "whspr" onto the screen title's baseline. That bottom-weighted position
/// still sits well clear of the macOS traffic lights (which float near the
/// band's top, ~y20) without needing top padding to dodge them.
const BRAND_PAD: iced::Padding = iced::Padding {
    top: 0.0,
    right: 20.0,
    bottom: spacing::layout::HEADER_TITLE_PAD_BOTTOM + HEADER_BASELINE_COMPENSATION,
    left: 20.0,
};

/// The rasterized window icon's side length, in lockstep with the constant
/// of the same name `build.rs` renders `assets/icon.svg` to.
const ICON_SIZE: u32 = 512;

/// The Hub window's icon (the "1c" mark). `build.rs` rasterizes
/// `assets/icon.svg` to flat RGBA8 at build time and writes it to
/// `OUT_DIR`; only the vector is committed to git (see the repo
/// `.gitignore`). `cargo run` without an app bundle has limited macOS dock-
/// icon support, but the window icon itself is correct today and will
/// carry over once whspr ships as a bundled `.app`.
fn window_icon() -> Option<iced::window::icon::Icon> {
    const ICON_RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba"));
    iced::window::icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE).ok()
}

/// The Hub window's settings. On macOS the system title bar is hidden and
/// made transparent with a full-size content view, so the app's own paper
/// ground and rounded corners reach the top edge and the traffic lights
/// float directly over the custom header (the Claude-desktop pattern).
/// `decorations` stays `true` -- that keeps the traffic lights and rounded
/// corners; only the title bar chrome is removed. The whole custom header
/// is the drag handle (see `Message::DragHubWindow`).
#[cfg(target_os = "macos")]
pub fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        icon: window_icon(),
        ..iced::window::Settings::default()
    }
}

/// On Windows the OS title bar -- and with it the system min/maximize/close
/// buttons and the resize border -- is removed with `decorations: false`, so
/// the Modernist paper header runs clean to the window's top edge, matching
/// the borderless macOS look. The app then draws its own caption controls
/// and wires window drag + edge resize itself (see `caption_windows`).
/// `CornerPreference::Round` keeps the Win11 rounded corners the removed
/// frame would otherwise have provided. The undecorated drop shadow is left
/// off deliberately: enabling it draws a thin 1px line across the top of the
/// window (documented winit behavior) that would break the seamless header.
#[cfg(target_os = "windows")]
pub fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        decorations: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            corner_preference: iced::window::settings::platform::CornerPreference::Round,
            ..iced::window::settings::PlatformSpecific::default()
        },
        icon: window_icon(),
        ..iced::window::Settings::default()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        icon: window_icon(),
        ..iced::window::Settings::default()
    }
}

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    let scheme = crate::theme::scheme(&state.theme);

    // The note desk replaces the whole nav-rail + header shell, so short-
    // circuit into its own full-screen view before assembling any of it.
    if let Some(nd) = &state.note_desk {
        return note_desk::view(state, nd, scheme);
    }

    let screen_content = match state.screen {
        Screen::Dictate => dictate::view(state, scheme),
        Screen::History => history::view(state, scheme),
        Screen::Models => models::view(state, scheme),
        Screen::Speakers => speakers::view(state, scheme),
        Screen::Settings => settings::view(state, scheme),
    };

    let body = scrollable(
        container(screen_content)
            .width(Length::Fill)
            .padding([spacing::XL, spacing::XXL]),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme, status| styles::scrollable::rail(scheme, status));

    // The whole header band is the window's drag handle (there is no system
    // title bar on macOS). Interactive children (the theme toggle) capture
    // their own presses first, so dragging only starts on the empty header.
    let header = mouse_area(screen_header(
        screen_title(state.screen),
        header_trailing(state, scheme),
        scheme,
    ))
    .on_press(Message::DragHubWindow);

    let main = column![header, error_banner(state, scheme), body,]
        .width(Length::Fill)
        .height(Length::Fill);

    let hub: Element<'_, Message> = container(
        row![
            nav_rail(state, scheme),
            widgets::vrule(spacing::layout::RULE, scheme),
            main
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .style(move |_theme| styles::container::surface(scheme))
    .into();

    // On Windows the borderless window (see `window_settings`) has no system
    // title bar, so overlay our own caption controls and resize hit-test
    // zones. macOS/Linux keep the OS chrome and return `hub` untouched.
    #[cfg(target_os = "windows")]
    let hub = caption_windows::chrome(hub, scheme);

    hub
}

/// The rail's label for a screen -- pure so the wording stays testable.
fn rail_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Dictate => "Dictate",
        Screen::History => "History",
        Screen::Models => "Models",
        Screen::Speakers => "Speakers",
        Screen::Settings => "Settings",
    }
}

/// A screen's header title.
fn screen_title(screen: Screen) -> &'static str {
    rail_label(screen)
}

/// The left numbered nav rail: a brand block, the five rail items, and a
/// bottom status block, all on the paper ground with a 2px right rule drawn
/// as a sibling in `view`.
fn nav_rail<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let items = column(
        SCREENS
            .into_iter()
            .enumerate()
            .map(|(i, screen)| rail_item(scheme, screen, i + 1, state.screen == screen)),
    )
    .spacing(spacing::XS);

    // The brand block is part of the draggable header (it sits under the
    // floating traffic lights on macOS).
    let brand = mouse_area(brand(scheme)).on_press(Message::DragHubWindow);

    container(
        column![
            brand,
            widgets::hr(scheme),
            container(items).padding([spacing::MD, 0.0]),
            Space::new().height(Length::Fill),
            widgets::hr(scheme),
            status_block(state, scheme),
        ]
        .width(Length::Fill),
    )
    .width(Length::Fixed(spacing::layout::RAIL_W))
    .height(Length::Fill)
    .style(move |_theme| styles::container::rail(scheme))
    .into()
}

/// The rail's brand block: a 12px accent square + the "whspr" wordmark, in
/// a `RAIL_HEADER_H`-tall band bottom-aligned to match the screen header,
/// plus a baseline compensation so the smaller wordmark's glyph baseline
/// lands on the larger screen title's (see `BRAND_PAD` and
/// `crate::theme::widgets::screen_header`) -- the two titles share a
/// baseline, not just a box bottom.
fn brand<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    container(
        row![
            widgets::status_square(widgets::Mark::Solid, 12.0, scheme),
            text("whspr")
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font())
                .color(scheme.on_surface),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(spacing::layout::RAIL_HEADER_H))
    .width(Length::Fill)
    .align_y(Alignment::End)
    .padding(BRAND_PAD)
    .into()
}

/// A single rail item: `NN`, the label, and a trailing 8px active square.
fn rail_item<'a>(
    scheme: &'static color::Scheme,
    screen: Screen,
    number: usize,
    active: bool,
) -> Element<'a, Message> {
    let label_color = if active {
        scheme.primary
    } else {
        scheme.on_surface
    };
    let num_color = if active {
        scheme.primary
    } else {
        scheme.on_surface_variant
    };
    let mark: Element<'a, Message> = if active {
        widgets::status_square(widgets::Mark::Solid, 8.0, scheme)
    } else {
        Space::new().width(Length::Fixed(8.0)).into()
    };

    let content = row![
        container(
            text(format!("{number:02}"))
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(num_color)
        )
        .width(Length::Fixed(24.0)),
        text(rail_label(screen))
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font())
            .color(label_color)
            .width(Length::Fill),
        mark,
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    button(content)
        .width(Length::Fill)
        .padding([10.0, 20.0])
        .on_press(Message::TabSelected(screen))
        .style(move |_theme, status| ghost_row(scheme, status))
        .into()
}

/// A flush-left, transparent row button with a faint ink hover -- the rail
/// item and Settings sub-nav share this.
pub(crate) fn ghost_row(scheme: &'static color::Scheme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.07))),
            ..base
        },
        _ => base,
    }
}

/// The rail's bottom status block: the pipeline word, the input device, and
/// the offline tag.
fn status_block<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let device = state
        .selected_device
        .clone()
        .unwrap_or_else(|| "No microphone".to_string());

    column![
        row![
            widgets::status_square(widgets::Mark::Ink, 8.0, scheme),
            text(pipeline_word(state.pipeline_state))
                .size(type_scale::LABEL_MEDIUM.size)
                .font(type_scale::LABEL_LARGE.font())
                .color(scheme.on_surface),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
        text(device)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
        widgets::tag(widgets::TagKind::Outline, "On this Mac · offline", scheme),
    ]
    .spacing(spacing::SM)
    .padding([spacing::LG, 20.0])
    .into()
}

/// A glanceable word for the pipeline's current state (shown on the rail).
fn pipeline_word(state: whspr_core::PipelineState) -> &'static str {
    match state {
        whspr_core::PipelineState::Idle => "Ready",
        whspr_core::PipelineState::Recording => "Listening",
        whspr_core::PipelineState::Transcribing => "Transcribing",
        whspr_core::PipelineState::Refining => "Cleaning up",
        whspr_core::PipelineState::Injecting => "Typing",
        whspr_core::PipelineState::Error => "Error",
    }
}

/// The theme-toggle button's label names the theme you'd switch *to*.
fn theme_toggle_label(theme: &iced::Theme) -> &'static str {
    match theme {
        iced::Theme::Dark => "Switch to light",
        _ => "Switch to dark",
    }
}

/// The screen header's trailing slot: the theme toggle (a ghost action).
fn header_trailing<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    button(
        text(theme_toggle_label(&state.theme))
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::text(scheme, status))
    .on_press(Message::ThemeToggled)
    .into()
}

/// A mono accent error notice under the header when a worker error exists.
fn error_banner<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match &state.last_error {
        Some(error) => container(
            container(
                text(format!("Last worker error: {error}"))
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font()),
            )
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(move |_theme| styles::container::error_banner(scheme)),
        )
        .padding([spacing::SM, spacing::XXL])
        .into(),
        None => Space::new().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_toggle_label_names_the_target_theme() {
        assert_eq!(theme_toggle_label(&iced::Theme::Dark), "Switch to light");
        assert_eq!(theme_toggle_label(&iced::Theme::Light), "Switch to dark");
    }

    #[test]
    fn every_rail_item_has_a_distinct_label() {
        let labels: Vec<&str> = SCREENS.iter().map(|&s| rail_label(s)).collect();
        for (i, a) in labels.iter().enumerate() {
            for b in &labels[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(rail_label(Screen::Dictate), "Dictate");
    }

    #[test]
    fn rail_is_numbered_in_screen_order() {
        assert_eq!(SCREENS[0], Screen::Dictate);
        assert_eq!(SCREENS[4], Screen::Settings);
    }

    #[test]
    fn pipeline_word_covers_every_state() {
        use whspr_core::PipelineState::*;
        for s in [Idle, Recording, Transcribing, Refining, Injecting, Error] {
            assert!(!pipeline_word(s).is_empty());
        }
    }
}
