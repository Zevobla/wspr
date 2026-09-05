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
