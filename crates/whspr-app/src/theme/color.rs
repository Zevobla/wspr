//! Modernist color tokens: the single source of truth for whspr-app's light
//! and dark color schemes, plus the small color-math helpers ("state
//! layers", disabled-state opacities, `lerp`) the interactive components are
//! built from.
//!
//! ## Role names are intentionally stable
//! The `Scheme` field names are carried over from the app's previous
//! Material-3 scheme so the ~20 style/screen call sites that read them keep
//! compiling through the Modernist restyle without a mechanical rename --
//! the *values* are what change. The mapping onto Modernist roles (see the
//! design guide / `design/_ds/modernist-*/styles.css`) is:
//!
//! | field | Modernist role | light value |
//! | --- | --- | --- |
//! | `primary` | the one accent | `#ec3013` (theme-constant) |
//! | `on_primary` | ground/paper, for text on the accent | `#f3f2f2` |
//! | `surface` | ground / paper (window background) | `#f3f2f2` |
//! | `on_surface` | ink | `#201e1d` |
//! | `on_surface_variant` | dimmed ink (captions, help) | `#605d5d` |
//! | `surface_container*` | the one tinted fill (cards, inputs) | `#eae9e9` |
//! | `outline`/`outline_variant` | the divider (ink @ 40%) | ink @ .40 |
//! | `error`/`on_error` | the accent again (mono scheme) | `#ec3013`/paper |
//! | `error_container`/`on_*` | accent-100 tint / accent-800 text | ramp |
//! | `secondary_container`/`on_*` | neutral-200 tint / ink | ramp |
//! | `tertiary`/`tertiary_container` | accent / neutral-200 track | ramp |
//! | `inverse_surface`/`on_*` | ink / paper (inverted surfaces) | swap |
//! | `success_container`/`on_*` | ink / paper (tray "Done", inverted) | swap |
//!
//! `neutral`/`accent_ramp` carry the full 100..900 tonal ramps (indexed
//! 0..8) for tags, tables and meters; `accent_hover`/`accent_pressed` are
//! the accent's one-step-darker (light) / one-step-lighter (dark) states.

use iced::Color;

/// A resolved set of Modernist color roles for one theme variant (light or
/// dark). See `LIGHT`/`DARK` for the values, the module doc for the role
/// mapping, and `crate::theme::scheme` for how a `Scheme` is picked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub surface_container: Color,
    pub surface_container_low: Color,
    pub surface_container_highest: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub success_container: Color,
    pub on_success_container: Color,
    /// The accent's hover state: one ramp step darker on the light ground,
    /// one step lighter on the dark ground (per the guide's interaction
    /// note). Used for `styles::button::primary`.
    pub accent_hover: Color,
    /// The accent's pressed state: two ramp steps from the base, same
    /// direction as `accent_hover`.
    pub accent_pressed: Color,
    /// The neutral 100..900 tonal ramp (index 0 = 100, lightest on light).
    pub neutral: [Color; 9],
    /// The accent 100..900 tonal ramp (index 0 = 100).
    pub accent_ramp: [Color; 9],
}

const INK: Color = Color::from_rgb8(0x20, 0x1E, 0x1D);
const PAPER: Color = Color::from_rgb8(0xF3, 0xF2, 0xF2);
const SURFACE_LIGHT: Color = Color::from_rgb8(0xEA, 0xE9, 0xE9);
const PAPER_DARK: Color = Color::from_rgb8(0x1A, 0x19, 0x18);
const SURFACE_DARK: Color = Color::from_rgb8(0x26, 0x24, 0x23);
/// The one accent, constant across both themes.
const ACCENT: Color = Color::from_rgb8(0xEC, 0x30, 0x13);

/// The divider: ink at 40% on the light ground.
const DIVIDER_LIGHT: Color = Color::from_rgba(
    0x20 as f32 / 255.0,
    0x1E as f32 / 255.0,
    0x1D as f32 / 255.0,
    0.40,
);
/// The divider: paper at 40% on the dark ground.
const DIVIDER_DARK: Color = Color::from_rgba(
    0xF3 as f32 / 255.0,
    0xF2 as f32 / 255.0,
    0xF2 as f32 / 255.0,
    0.40,
);

const NEUTRAL_LIGHT: [Color; 9] = [
    Color::from_rgb8(0xF8, 0xF4, 0xF4),
    Color::from_rgb8(0xEA, 0xE7, 0xE7),
    Color::from_rgb8(0xD7, 0xD3, 0xD3),
    Color::from_rgb8(0xBA, 0xB6, 0xB6),
    Color::from_rgb8(0x9B, 0x97, 0x97),
    Color::from_rgb8(0x7D, 0x79, 0x79),
    Color::from_rgb8(0x60, 0x5D, 0x5D),
    Color::from_rgb8(0x44, 0x41, 0x41),
    Color::from_rgb8(0x2D, 0x2B, 0x2B),
];
const NEUTRAL_DARK: [Color; 9] = [
    Color::from_rgb8(0x2D, 0x2B, 0x2B),
    Color::from_rgb8(0x44, 0x41, 0x41),
    Color::from_rgb8(0x60, 0x5D, 0x5D),
    Color::from_rgb8(0x7D, 0x79, 0x79),
    Color::from_rgb8(0x9B, 0x97, 0x97),
    Color::from_rgb8(0xBA, 0xB6, 0xB6),
    Color::from_rgb8(0xD7, 0xD3, 0xD3),
    Color::from_rgb8(0xEA, 0xE7, 0xE7),
    Color::from_rgb8(0xF8, 0xF4, 0xF4),
];
const ACCENT_LIGHT: [Color; 9] = [
    Color::from_rgb8(0xFF, 0xF2, 0xEF),
    Color::from_rgb8(0xFF, 0xE0, 0xD9),
    Color::from_rgb8(0xFF, 0xC4, 0xB8),
    Color::from_rgb8(0xFF, 0x97, 0x83),
    Color::from_rgb8(0xFF, 0x56, 0x3C),
    Color::from_rgb8(0xDD, 0x2B, 0x0F),
    Color::from_rgb8(0xAE, 0x18, 0x00),
    Color::from_rgb8(0x7C, 0x14, 0x05),
    Color::from_rgb8(0x4D, 0x17, 0x0E),
];
const ACCENT_DARK: [Color; 9] = [
    Color::from_rgb8(0x4D, 0x17, 0x0E),
    Color::from_rgb8(0x7C, 0x14, 0x05),
    Color::from_rgb8(0xAE, 0x18, 0x00),
    Color::from_rgb8(0xDD, 0x2B, 0x0F),
    Color::from_rgb8(0xFF, 0x56, 0x3C),
    Color::from_rgb8(0xFF, 0x97, 0x83),
    Color::from_rgb8(0xFF, 0xC4, 0xB8),
    Color::from_rgb8(0xFF, 0xE0, 0xD9),
    Color::from_rgb8(0xFF, 0xF2, 0xEF),
];

/// The Modernist light scheme: near-mono red on paper.
pub const LIGHT: Scheme = Scheme {
    primary: ACCENT,
    on_primary: PAPER,
    primary_container: ACCENT_LIGHT[0],
    secondary_container: NEUTRAL_LIGHT[1],
    on_secondary_container: INK,
    tertiary: ACCENT,
    tertiary_container: NEUTRAL_LIGHT[1],
    on_tertiary_container: INK,
    error: ACCENT,
    on_error: PAPER,
    error_container: ACCENT_LIGHT[0],
    on_error_container: ACCENT_LIGHT[7],
    surface: PAPER,
    on_surface: INK,
    on_surface_variant: NEUTRAL_LIGHT[6],
    surface_container: SURFACE_LIGHT,
    surface_container_low: SURFACE_LIGHT,
    surface_container_highest: SURFACE_LIGHT,
    outline: DIVIDER_LIGHT,
    outline_variant: DIVIDER_LIGHT,
    inverse_surface: INK,
    inverse_on_surface: PAPER,
    success_container: INK,
    on_success_container: PAPER,
    accent_hover: ACCENT_LIGHT[5],
    accent_pressed: ACCENT_LIGHT[6],
    neutral: NEUTRAL_LIGHT,
    accent_ramp: ACCENT_LIGHT,
};

/// The Modernist dark scheme: the ramps invert, the red is unchanged.
pub const DARK: Scheme = Scheme {
    primary: ACCENT,
    on_primary: PAPER_DARK,
    primary_container: ACCENT_DARK[0],
    secondary_container: NEUTRAL_DARK[1],
    on_secondary_container: PAPER,
    tertiary: ACCENT,
    tertiary_container: NEUTRAL_DARK[1],
    on_tertiary_container: PAPER,
    error: ACCENT,
    on_error: PAPER_DARK,
    error_container: ACCENT_DARK[0],
    on_error_container: ACCENT_DARK[6],
    surface: PAPER_DARK,
    on_surface: PAPER,
    on_surface_variant: NEUTRAL_DARK[5],
    surface_container: SURFACE_DARK,
    surface_container_low: SURFACE_DARK,
    surface_container_highest: SURFACE_DARK,
    outline: DIVIDER_DARK,
    outline_variant: DIVIDER_DARK,
    inverse_surface: PAPER,
    inverse_on_surface: PAPER_DARK,
    success_container: PAPER,
    on_success_container: PAPER_DARK,
    accent_hover: ACCENT_DARK[4],
    accent_pressed: ACCENT_DARK[5],
    neutral: NEUTRAL_DARK,
    accent_ramp: ACCENT_DARK,
};

/// Hover state-layer opacity for interactive components that layer a wash
/// of the foreground color over their resting fill.
pub const HOVER_STATE_OPACITY: f32 = 0.08;
/// Pressed (or menu-open) state-layer opacity -- see `HOVER_STATE_OPACITY`.
pub const PRESSED_STATE_OPACITY: f32 = 0.10;
/// Disabled-container opacity: a disabled component's fill/border is
/// `on_surface` at this opacity.
pub const DISABLED_CONTAINER_OPACITY: f32 = 0.12;
/// Disabled-content opacity: a disabled component's text/icon is
/// `on_surface` at this opacity (Modernist's 45% is close; this stays for
/// call-site stability).
pub const DISABLED_CONTENT_OPACITY: f32 = 0.38;

/// Linearly interpolates every channel (including alpha) from `from` to
/// `to` at `t` (`0.0` = `from`, `1.0` = `to`). The shared primitive behind
/// `state_layer`.
pub fn lerp(from: Color, to: Color, t: f32) -> Color {
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

/// Overlays `on_color` onto `base` at `opacity`, keeping `base`'s own alpha
/// (a state layer tints a fill, it doesn't fade it).
pub fn state_layer(base: Color, on_color: Color, opacity: f32) -> Color {
    Color {
        a: base.a,
        ..lerp(base, on_color, opacity)
    }
}

/// A translucent wash of `on_color` alone, for components with no resting
/// fill to layer onto (outlined/text buttons), or for dimming a color by a
/// flat opacity (the disabled-state treatment).
pub fn wash(on_color: Color, opacity: f32) -> Color {
    Color {
        a: opacity,
        ..on_color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_is_theme_constant() {
        assert_eq!(LIGHT.primary, DARK.primary);
        assert_eq!(LIGHT.primary, ACCENT);
    }

    #[test]
    fn ground_inverts_between_themes() {
        assert_ne!(LIGHT.surface, DARK.surface);
        assert_ne!(LIGHT.on_surface, DARK.on_surface);
    }

    #[test]
    fn ramps_have_nine_steps_and_invert() {
        assert_eq!(LIGHT.neutral.len(), 9);
        // The dark ramp is the light ramp reversed.
        assert_eq!(LIGHT.neutral[0], DARK.neutral[8]);
        assert_eq!(LIGHT.accent_ramp[0], DARK.accent_ramp[8]);
    }

    #[test]
    fn lerp_at_zero_is_from_and_at_one_is_to() {
        let from = Color::from_rgb8(0, 0, 0);
        let to = Color::from_rgb8(255, 255, 255);
        assert_eq!(lerp(from, to, 0.0), from);
        assert_eq!(lerp(from, to, 1.0), to);
    }

    #[test]
    fn state_layer_keeps_base_alpha() {
        let base = Color {
            a: 0.5,
            ..LIGHT.primary
        };
        let layered = state_layer(base, LIGHT.on_primary, HOVER_STATE_OPACITY);
        assert_eq!(layered.a, base.a);
        assert_ne!(layered, base);
    }

    #[test]
    fn wash_sets_the_requested_opacity() {
        let washed = wash(LIGHT.on_surface, DISABLED_CONTENT_OPACITY);
        assert_eq!(washed.a, DISABLED_CONTENT_OPACITY);
        assert_eq!(washed.r, LIGHT.on_surface.r);
    }
}
