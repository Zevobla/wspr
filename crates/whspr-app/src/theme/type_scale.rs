//! MD3's type scale, trimmed to the roles whspr-app's Hub/Flow Bar
//! actually use. See the material-3 skill's typography reference for the
//! full 15-style scale and the "emphasized" variants (`TypeStyle::
//! emphasized`) this trims from.
//!
//! Operate-mode product UI doesn't need MD3's Display/Headline sizes
//! (those are for marketing-scale hero text) or a wide type-scale ratio --
//! a single family across title/label/body at a tight scale reads calmer
//! at desktop-app density than the full mobile-first scale would.

use iced::font::Weight;
use iced::Font;

use crate::theme::fonts;

/// One MD3 type-scale entry: a size plus the weight it's set in. iced has
/// no separate line-height/tracking knobs on `Font` (those live on the
/// `Text` widget and default to something reasonable at this desktop-app
/// scale), so `TypeStyle` only carries what the type role actually changes
/// here.
#[derive(Debug, Clone, Copy)]
pub struct TypeStyle {
    pub size: f32,
    pub weight: Weight,
}

impl TypeStyle {
    const fn new(size: f32, weight: Weight) -> Self {
        Self { size, weight }
    }

    /// The `iced::Font` this style renders with: the Archivo family (see
    /// `crate::theme::fonts`) at this style's weight.
    pub fn font(&self) -> Font {
        Font {
            weight: self.weight,
            ..fonts::DEFAULT
        }
    }

    /// MD3's "emphasized" variant of this style: same size, one weight
    /// step heavier (see the skill's Emphasized Type Styles notes). Used
    /// for the Flow Bar's glanceable state label -- the one place in this
    /// app where that extra emphasis earns its keep.
    pub const fn emphasized(self) -> Self {
        let weight = match self.weight {
            Weight::Thin => Weight::ExtraLight,
            Weight::ExtraLight => Weight::Light,
            Weight::Light => Weight::Normal,
            Weight::Normal => Weight::Medium,
            Weight::Medium => Weight::Semibold,
            Weight::Semibold => Weight::Bold,
            Weight::Bold => Weight::ExtraBold,
            Weight::ExtraBold | Weight::Black => Weight::Black,
        };
        Self { weight, ..self }
    }
}

/// A screen's `<h2>` title in the header band ("Dictate", "Settings").
pub const TITLE_LARGE: TypeStyle = TypeStyle::new(28.0, Weight::ExtraBold);
/// A section head (`<h4>`) inside a screen body ("Cleanup", "Recent").
pub const TITLE_MEDIUM: TypeStyle = TypeStyle::new(18.0, Weight::ExtraBold);
/// Field captions above a control, and de-emphasized help text.
pub const LABEL_MEDIUM: TypeStyle = TypeStyle::new(12.0, Weight::Normal);
/// Control / row labels and button text (flush-left).
pub const LABEL_LARGE: TypeStyle = TypeStyle::new(14.0, Weight::Semibold);
/// General body copy and table cells.
pub const BODY_MEDIUM: TypeStyle = TypeStyle::new(13.0, Weight::Normal);

/// The hero display size: the Dictate timer, onboarding statements.
pub const DISPLAY: TypeStyle = TypeStyle::new(44.0, Weight::ExtraBold);
/// A large stat number (History's dictations / words / wpm strip).
pub const STAT: TypeStyle = TypeStyle::new(32.0, Weight::ExtraBold);
/// The on-screen transcript body: large, quiet, readable.
pub const TRANSCRIPT: TypeStyle = TypeStyle::new(22.0, Weight::Normal);
/// An uppercase kicker / table header / rail number: small, tracked caps.
/// Callers uppercase the string themselves (iced has no `text-transform`).
pub const KICKER: TypeStyle = TypeStyle::new(11.0, Weight::Semibold);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasized_steps_up_exactly_one_weight() {
        assert_eq!(
            TypeStyle::new(14.0, Weight::Normal).emphasized().weight,
            Weight::Medium
        );
        assert_eq!(
            TypeStyle::new(14.0, Weight::Medium).emphasized().weight,
            Weight::Semibold
        );
    }

    #[test]
    fn emphasized_preserves_size() {
        let style = TITLE_LARGE.emphasized();
        assert_eq!(style.size, TITLE_LARGE.size);
    }

    #[test]
    fn emphasized_saturates_at_black() {
        assert_eq!(
            TypeStyle::new(14.0, Weight::Black).emphasized().weight,
            Weight::Black
        );
    }
}
