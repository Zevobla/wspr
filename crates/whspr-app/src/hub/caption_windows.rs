//! Windows-only custom window chrome: the caption controls (minimize /
//! maximize-restore / close) drawn in the Modernist visual language.
//!
//! On Windows the Hub opens borderless (`decorations:false`, see
//! `super::window_settings`), so the OS provides no title bar, no
//! min/maximize/close buttons and no resize border -- the app draws and
//! wires all of it. This module is compiled and referenced *only* on Windows
//! (`#[cfg(target_os = "windows")]` at the `mod` site), so macOS and Linux
//! chrome stay byte-for-byte unchanged.
//!
//! The controls sit flush in the window's top-right corner, overlaid on the
//! existing paper header via a `stack`: the header underneath is untouched,
//! and the overlay is transparent and event-ignoring everywhere except the
//! buttons, so the header's own drag handle (`Message::DragHubWindow`) still
//! receives presses in the gaps. Each control is a squared, zero-radius
//! button matching the geometric status marks -- a 2px ink glyph on
//! transparent paper that reddens to the accent on hover; the close button
//! additionally fills red with a paper glyph, the one universally read
//! destructive cue.

use iced::widget::svg::{self, Handle};
use iced::widget::{button, container, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::state::Message;
use crate::theme::color;

/// Caption button footprint. 46x32 echoes the Windows system caption metrics
/// so the controls land where muscle memory expects, while fitting inside
/// the header band (`spacing::layout::HEADER_H` = 72).
const BTN_W: f32 = 46.0;
const BTN_H: f32 = 32.0;

/// The three squared 2px glyphs, authored inline (not vendored under
/// `assets/icons/`) since they're Windows-caption-only. Squared line caps
/// and zero corner radius keep them in the Modernist geometric language;
/// `currentColor` lets the svg widget recolor each one per hover state. The
/// marks are drawn 8..16 inside a 24-unit viewBox so they render small and
/// centered while the svg element itself fills the whole button (padding 0),
/// keeping the glyph's hover region and the button's exactly aligned.
const MINIMIZE_GLYPH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><line x1="8" y1="12" x2="16" y2="12" stroke="currentColor" stroke-width="2"/></svg>"#;
const MAXIMIZE_GLYPH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="8" y="8" width="8" height="8" fill="none" stroke="currentColor" stroke-width="2"/></svg>"#;
const CLOSE_GLYPH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><line x1="8" y1="8" x2="16" y2="16" stroke="currentColor" stroke-width="2" stroke-linecap="square"/><line x1="16" y1="8" x2="8" y2="16" stroke="currentColor" stroke-width="2" stroke-linecap="square"/></svg>"#;

/// Whether a caption button is the destructive "close" (red fill on hover)
/// or a regular control (only its mark reddens on hover).
#[derive(Clone, Copy)]
enum Kind {
    Regular,
    Close,
}

/// Overlays the Windows caption controls on top of the Hub `content`.
/// `content` renders unchanged underneath; the caption's empty gaps are
/// transparent and pass presses through to the header's drag handle beneath.
pub fn chrome<'a>(
    content: Element<'a, Message>,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    stack![content, caption_bar(scheme)].into()
}

/// The caption controls, pinned flush to the window's top-right corner
/// inside a full-window, transparent, event-ignoring container.
fn caption_bar<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    let controls = row![
        caption_button(
            MINIMIZE_GLYPH,
            Message::MinimizeHubWindow,
            Kind::Regular,
            scheme
        ),
        caption_button(
            MAXIMIZE_GLYPH,
            Message::ToggleMaximizeHubWindow,
            Kind::Regular,
            scheme
        ),
        caption_button(CLOSE_GLYPH, Message::CloseHubWindow, Kind::Close, scheme),
    ];
    container(controls)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::Start)
        .into()
}

/// One caption control: a squared, zero-radius button whose svg glyph fills
/// it exactly (padding 0), so the button's hover region and the glyph's
/// coincide -- the button paints the background (transparent, or red for
/// `Close`) while the glyph recolors itself off its own hover status.
fn caption_button<'a>(
    glyph: &'static [u8],
    on_press: Message,
    kind: Kind,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let mark = svg::Svg::new(Handle::from_memory(glyph))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme, status| svg::Style {
            color: Some(mark_color(kind, scheme, status)),
        });
    button(mark)
        .width(Length::Fixed(BTN_W))
        .height(Length::Fixed(BTN_H))
        .padding(0)
        .on_press(on_press)
        .style(move |_theme, status| caption_style(kind, scheme, status))
        .into()
}

/// The glyph color: ink at rest, the red accent on hover; on the close
/// button's red hover fill it flips to paper so the mark stays legible.
fn mark_color(kind: Kind, scheme: &color::Scheme, status: svg::Status) -> Color {
    match (kind, status) {
        (Kind::Close, svg::Status::Hovered) => scheme.on_primary,
        (_, svg::Status::Hovered) => scheme.primary,
        _ => scheme.on_surface,
    }
}

/// The button background: transparent at rest; a faint ink wash on hover for
/// the regular controls (matching the rail's `ghost_row`), a solid accent
/// fill for close.
fn caption_style(kind: Kind, scheme: &color::Scheme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    };
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    match (kind, hovered) {
        (Kind::Close, true) => button::Style {
            background: Some(Background::Color(scheme.primary)),
            ..base
        },
        (Kind::Regular, true) => button::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.07))),
            ..base
        },
        (_, false) => base,
    }
}
