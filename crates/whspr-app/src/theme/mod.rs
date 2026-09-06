//! Material 3 design tokens for whspr-app: color, shape, spacing, type
//! scale, and motion, plus the iced widget `style` functions built on top
//! of them (see `styles`). One source of truth every Hub/Flow Bar surface
//! styles from -- iced isn't the web, so there's no `--md-sys-*` custom
//! property layer to lean on; these modules translate the MD3 spec into
//! iced's `Theme`/`Style`/`Border`/`Color` model directly.

pub mod color;
pub mod motion;
pub mod shape;
pub mod spacing;
pub mod styles;
pub mod type_scale;

pub use color::Scheme;

/// The MD3 color scheme for the app's current theme. whspr-app's Hub only
/// ever toggles `state.theme` between `iced::Theme::Light`/`Theme::Dark`
/// (see `crate::app`'s `ThemeToggled` handling), so anything else falls
/// back to `color::LIGHT` -- the same "anything that isn't Dark is treated
/// as Light" rule that toggle itself uses.
pub fn scheme(theme: &iced::Theme) -> &'static Scheme {
    match theme {
        iced::Theme::Dark => &color::DARK,
        _ => &color::LIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_returns_dark_for_dark_theme() {
        let dark_theme = iced::Theme::Dark;
        let result = scheme(&dark_theme);
        assert_eq!(result.primary, color::DARK.primary);
    }

    #[test]
    fn scheme_returns_light_for_light_theme() {
        let light_theme = iced::Theme::Light;
        let result = scheme(&light_theme);
        assert_eq!(result.primary, color::LIGHT.primary);
    }

    #[test]
    fn scheme_returns_light_as_fallback() {
        let dark = scheme(&iced::Theme::Dark);
        let light = scheme(&iced::Theme::Light);
        assert_ne!(dark.primary, light.primary);
    }
}
