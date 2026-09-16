//! The Modernist design tokens the installer is built from -- the same
//! language as `whspr-app`'s `crate::theme::color`/`type_scale`, pared down
//! to just what the four installer screens need.
//!
//! There is one accent, one ink, one paper, 2px rules everywhere, zero
//! corner radius, and no gradients. Type is Archivo in three static weights
//! (Regular 400 / SemiBold 600 / ExtraBold 800), embedded from `OUT_DIR` by
//! the same `build.rs` + `ARCHIVO_DIR` mechanism the app uses.

use iced::font::{Family, Stretch, Style, Weight};
use iced::widget::button;
use iced::{Background, Border, Color, Font};

// ---------------------------------------------------------------------------
// Color tokens (see `whspr-app/src/theme/color.rs`).
// ---------------------------------------------------------------------------

/// The one accent: Modernist red.
pub const ACCENT: Color = Color::from_rgb8(0xEC, 0x30, 0x13);
/// The accent one ramp-step darker -- primary buttons' hover fill.
pub const ACCENT_HOVER: Color = Color::from_rgb8(0xDD, 0x2B, 0x0F);
/// The accent two ramp-steps darker -- primary buttons' pressed fill.
pub const ACCENT_PRESSED: Color = Color::from_rgb8(0xAE, 0x18, 0x00);
/// Ink: the near-black used for text and 2px rules on paper.
pub const INK: Color = Color::from_rgb8(0x20, 0x1E, 0x1D);
/// Paper: the window ground, and text on the accent.
pub const PAPER: Color = Color::from_rgb8(0xF3, 0xF2, 0xF2);
/// Dimmed ink: captions and de-emphasized help text.
pub const DIMMED: Color = Color::from_rgb8(0x60, 0x5D, 0x5D);
/// Neutral-300: the progress track and the light 1px divider between rows.
pub const NEUTRAL_300: Color = Color::from_rgb8(0xD4, 0xD2, 0xD1);

/// The strong divider: ink at 40% alpha.
pub const DIVIDER: Color = Color::from_rgba(
    0x20 as f32 / 255.0,
    0x1E as f32 / 255.0,
    0x1D as f32 / 255.0,
    0.40,
);
/// The header rule drawn on the red band: paper (white) at 40% alpha.
pub const WHITE_RULE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.40);
/// Paper at 65% alpha -- the header band's quieter second caption line.
pub const PAPER_MUTED: Color = Color::from_rgba(
    0xF3 as f32 / 255.0,
    0xF2 as f32 / 255.0,
    0xF2 as f32 / 255.0,
    0.72,
);

// ---------------------------------------------------------------------------
// Fonts (see `whspr-app/src/theme/fonts.rs`).
// ---------------------------------------------------------------------------

/// Archivo Regular (400) -- body copy and captions.
pub const ARCHIVO_REGULAR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/Archivo-Regular.ttf"));
/// Archivo SemiBold (600) -- labels, button text, footer.
pub const ARCHIVO_SEMIBOLD: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/Archivo-SemiBold.ttf"));
/// Archivo ExtraBold (800) -- titles, section heads, the wordmark.
pub const ARCHIVO_EXTRABOLD: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/Archivo-ExtraBold.ttf"));

/// The shared typographic family name all three faces register under.
pub const ARCHIVO: &str = "Archivo";

/// The default font: Archivo Regular. Passed to the daemon builder's
/// `.default_font(..)`; every other weight is built off this.
pub const DEFAULT: Font = Font {
    family: Family::Name(ARCHIVO),
    weight: Weight::Normal,
    stretch: Stretch::Normal,
    style: Style::Normal,
};

/// Archivo Regular (400).
pub fn regular() -> Font {
    DEFAULT
}

/// Archivo SemiBold (600).
pub fn semibold() -> Font {
    Font {
        weight: Weight::Semibold,
        ..DEFAULT
    }
}

/// Archivo ExtraBold (800).
pub fn extrabold() -> Font {
    Font {
        weight: Weight::ExtraBold,
        ..DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Button styles (see `whspr-app/src/theme/styles/button.rs`).
// ---------------------------------------------------------------------------

/// The primary button: a solid accent fill with paper text and zero radius;
/// hover/press step the accent one/two ramp steps darker (never a wash).
pub fn primary(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(ACCENT)),
        text_color: PAPER,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    };
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(ACCENT_HOVER)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(ACCENT_PRESSED)),
            ..base
        },
        _ => base,
    }
}

/// The ghost button: transparent with a 2px ink border and ink text; a
/// faint ink wash on hover.
pub fn ghost(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: None,
        text_color: INK,
        border: Border {
            color: INK,
            width: 2.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(wash(INK, 0.07))),
            ..base
        },
        _ => base,
    }
}

/// A translucent wash of `color` at `opacity` -- for hover state layers and
/// the header band's quieter text.
pub fn wash(color: Color, opacity: f32) -> Color {
    Color {
        a: opacity,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_ink_paper_are_the_modernist_tokens() {
        assert_eq!(ACCENT, Color::from_rgb8(0xEC, 0x30, 0x13));
        assert_eq!(INK, Color::from_rgb8(0x20, 0x1E, 0x1D));
        assert_eq!(PAPER, Color::from_rgb8(0xF3, 0xF2, 0xF2));
    }

    #[test]
    fn all_three_faces_are_bundled_truetype() {
        for face in [ARCHIVO_REGULAR, ARCHIVO_SEMIBOLD, ARCHIVO_EXTRABOLD] {
            assert!(face.len() > 10_000, "a real face, not a placeholder");
            assert_eq!(&face[0..4], b"\x00\x01\x00\x00");
        }
    }

    #[test]
    fn primary_button_is_accent_filled_and_square() {
        let style = primary(&iced::Theme::Light, button::Status::Active);
        assert_eq!(style.background, Some(Background::Color(ACCENT)));
        assert_eq!(style.text_color, PAPER);
        assert_eq!(style.border.radius, 0.0.into());
    }

    #[test]
    fn ghost_button_has_a_2px_ink_border_and_no_fill() {
        let style = ghost(&iced::Theme::Light, button::Status::Active);
        assert!(style.background.is_none());
        assert_eq!(style.border.width, 2.0);
        assert_eq!(style.border.color, INK);
    }
}
