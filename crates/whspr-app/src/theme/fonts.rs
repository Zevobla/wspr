//! Vendored font assets and the iced `Font`s the app renders with.
//!
//! Modernist is set entirely in Archivo (see `crate::theme`'s module doc).
//! We vendor the OFL-licensed Archivo as a single *variable* face carrying
//! the whole `wght` axis (Google Fonts ships Archivo only as a variable
//! font now); cosmic-text resolves the named weights the type scale asks
//! for -- Normal (400), Semibold (600), ExtraBold (800) -- along that axis,
//! so one `.font()` load at the daemon builder covers every weight. See
//! `crate::app::run` for the load and `crate::theme::type_scale` for how a
//! `TypeStyle` selects its weight under this family.

use iced::font::{Family, Stretch, Style, Weight};
use iced::Font;

/// The Archivo variable font (OFL). Loaded once via the daemon builder's
/// `.font(..)`; see the module doc for why a single variable face is enough.
pub const ARCHIVO_TTF: &[u8] = include_bytes!("../../assets/fonts/Archivo.ttf");

/// The family name Archivo registers under. Every `TypeStyle` selects this
/// family so all app text resolves to the one loaded face.
pub const ARCHIVO: &str = "Archivo";

/// The app's default font: Archivo at the regular weight. Passed to the
/// daemon builder's `.default_font(..)` so untyped `text(..)` still renders
/// in Archivo, and used as the base every `TypeStyle::font` builds on.
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
    fn archivo_font_is_bundled() {
        // A real TTF, not an empty placeholder -- guards against a missing
        // asset silently shipping.
        assert!(ARCHIVO_TTF.len() > 10_000);
        assert_eq!(&ARCHIVO_TTF[0..4], b"\x00\x01\x00\x00");
    }

    #[test]
    fn default_font_is_archivo() {
        assert!(matches!(DEFAULT.family, Family::Name(ARCHIVO)));
        assert_eq!(DEFAULT.weight, Weight::Normal);
    }
}
