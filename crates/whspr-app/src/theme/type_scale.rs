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

    /// The `iced::Font` this style renders with.
    pub fn font(&self) -> Font {
        Font {
            weight: self.weight,
            ..Font::DEFAULT
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

/// Top app bar / Hub window title ("whspr").
pub const TITLE_LARGE: TypeStyle = TypeStyle::new(22.0, Weight::Normal);
/// Section headers ("Settings", "Microphone", ...).
pub const TITLE_MEDIUM: TypeStyle = TypeStyle::new(16.0, Weight::Medium);
/// Field captions above a pick list or text input.
pub const LABEL_MEDIUM: TypeStyle = TypeStyle::new(12.0, Weight::Medium);
/// Button labels.
pub const LABEL_LARGE: TypeStyle = TypeStyle::new(14.0, Weight::Medium);
/// General body copy: descriptions, statuses, list rows.
pub const BODY_MEDIUM: TypeStyle = TypeStyle::new(14.0, Weight::Normal);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasized_steps_up_exactly_one_weight() {
        assert_eq!(TypeStyle::new(14.0, Weight::Normal).emphasized().weight, Weight::Medium);
        assert_eq!(TypeStyle::new(14.0, Weight::Medium).emphasized().weight, Weight::Semibold);
    }

    #[test]
    fn emphasized_preserves_size() {
        let style = TITLE_LARGE.emphasized();
        assert_eq!(style.size, TITLE_LARGE.size);
    }

    #[test]
    fn emphasized_saturates_at_black() {
        assert_eq!(TypeStyle::new(14.0, Weight::Black).emphasized().weight, Weight::Black);
    }
}
