//! Vendored font assets and the iced `Font`s the app renders with.
//!
//! Modernist is set entirely in Archivo (see `crate::theme`'s module doc).
//! We vendor the OFL-licensed Archivo as **static per-weight faces** --
//! Regular (400), SemiBold (600), ExtraBold (800) -- rather than the
//! variable font: cosmic-text/glyphon miscomputed glyph advances for the
//! Regular *instance* of the `wght`-axis variable TTF, ghosting body text.
//! The static faces carry baked-in advances and render crisply.
//!
//! All three share the typographic family name "Archivo" (name ID 16) and
//! differ by their OS/2 weight class, so `Font { family: Name("Archivo"),
//! weight }` (see `crate::theme::type_scale::TypeStyle::font`) resolves to
//! the correct static file per weight -- no axis interpolation. Each face
//! is registered with its own `.font(..)` call at the daemon builder (see
//! `crate::app::run`).

use iced::font::{Family, Stretch, Style, Weight};
use iced::Font;

/// Archivo Regular (400) -- body text, captions, table cells.
pub const ARCHIVO_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Archivo-Regular.ttf");
/// Archivo SemiBold (600) -- control/row labels and button text.
pub const ARCHIVO_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Archivo-SemiBold.ttf");
/// Archivo ExtraBold (800) -- screen titles, section heads, the wordmark.
pub const ARCHIVO_EXTRABOLD: &[u8] = include_bytes!("../../assets/fonts/Archivo-ExtraBold.ttf");

/// The shared typographic family name all three faces register under.
pub const ARCHIVO: &str = "Archivo";

/// The app's default font: Archivo at the regular weight. Passed to the
/// daemon builder's `.default_font(..)` and used as the base every
/// `TypeStyle::font` builds on (swapping only the weight).
pub const DEFAULT: Font = Font {
    family: Family::Name(ARCHIVO),
    weight: Weight::Normal,
    stretch: Stretch::Normal,
    style: Style::Normal,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_static_faces_are_bundled_truetype() {
        for face in [ARCHIVO_REGULAR, ARCHIVO_SEMIBOLD, ARCHIVO_EXTRABOLD] {
            assert!(face.len() > 10_000, "a real face, not an empty placeholder");
            // TrueType sfnt version tag.
            assert_eq!(&face[0..4], b"\x00\x01\x00\x00");
        }
    }

    #[test]
    fn default_font_is_archivo_regular() {
        assert!(matches!(DEFAULT.family, Family::Name(ARCHIVO)));
        assert_eq!(DEFAULT.weight, Weight::Normal);
    }
}
