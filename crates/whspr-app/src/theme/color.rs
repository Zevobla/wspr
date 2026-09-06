//! MD3 color tokens: the single source of truth for whspr-app's light and
//! dark color schemes, plus the small color-math helpers ("state layers",
//! disabled-state opacities) MD3's interactive components are built from.
//!
//! Values are the Material 3 baseline scheme (seed color `#6750A4`) from
//! the spec's default token tables, trimmed to the roles this app actually
//! reads -- see `Scheme`'s field docs for where each one is used
//! (`crate::hub`, `crate::flow_bar`, and `styles`). Two roles
//! (`success_container`/`on_success_container`) aren't part of the
//! official M3 role set -- M3 doesn't ship a built-in "success" pair -- so
//! they're added as a same-shape custom pair (mirroring the tonal
//! relationship of the official container roles) for the Flow Bar's "Done"
//! state; see `crate::flow_bar::base_colors_for`.

use iced::Color;

/// A resolved set of MD3 color roles for one theme variant (light or
/// dark). See `LIGHT`/`DARK` for the values and `crate::theme::scheme` for
/// how a `Scheme` is picked from the app's active `iced::Theme`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheme {
    pub primary: Color,
    pub on_primary: Color,
    /// Used as a light selection-highlight tint (text input/pick_list),
    /// not as a `primary_container`+`on_primary_container` fill/text pair.
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
    /// Custom addition, not an official M3 role -- see module docs.
    pub success_container: Color,
    /// Custom addition, not an official M3 role -- see module docs.
    pub on_success_container: Color,
}

/// The MD3 baseline light scheme.
pub const LIGHT: Scheme = Scheme {
    primary: Color::from_rgb8(0x67, 0x50, 0xA4),
    on_primary: Color::from_rgb8(0xFF, 0xFF, 0xFF),
    primary_container: Color::from_rgb8(0xEA, 0xDD, 0xFF),
    secondary_container: Color::from_rgb8(0xE8, 0xDE, 0xF8),
    on_secondary_container: Color::from_rgb8(0x1D, 0x19, 0x2B),
    tertiary: Color::from_rgb8(0x7D, 0x52, 0x60),
    tertiary_container: Color::from_rgb8(0xFF, 0xD8, 0xE4),
    on_tertiary_container: Color::from_rgb8(0x31, 0x11, 0x1D),
    error: Color::from_rgb8(0xB3, 0x26, 0x1E),
    on_error: Color::from_rgb8(0xFF, 0xFF, 0xFF),
    error_container: Color::from_rgb8(0xF9, 0xDE, 0xDC),
    on_error_container: Color::from_rgb8(0x41, 0x0E, 0x0B),
    surface: Color::from_rgb8(0xFE, 0xF7, 0xFF),
    on_surface: Color::from_rgb8(0x1D, 0x1B, 0x20),
    on_surface_variant: Color::from_rgb8(0x49, 0x45, 0x4F),
    surface_container: Color::from_rgb8(0xF3, 0xED, 0xF7),
    surface_container_low: Color::from_rgb8(0xF7, 0xF2, 0xFA),
    surface_container_highest: Color::from_rgb8(0xE6, 0xE0, 0xE9),
    outline: Color::from_rgb8(0x79, 0x74, 0x7E),
    outline_variant: Color::from_rgb8(0xCA, 0xC4, 0xD0),
    inverse_surface: Color::from_rgb8(0x32, 0x2F, 0x35),
    inverse_on_surface: Color::from_rgb8(0xF5, 0xEF, 0xF7),
    success_container: Color::from_rgb8(0xC8, 0xE6, 0xC9),
    on_success_container: Color::from_rgb8(0x1B, 0x5E, 0x20),
};

/// The MD3 baseline dark scheme.
pub const DARK: Scheme = Scheme {
    primary: Color::from_rgb8(0xD0, 0xBC, 0xFF),
    on_primary: Color::from_rgb8(0x38, 0x1E, 0x72),
    primary_container: Color::from_rgb8(0x4F, 0x37, 0x8B),
    secondary_container: Color::from_rgb8(0x4A, 0x44, 0x58),
    on_secondary_container: Color::from_rgb8(0xE8, 0xDE, 0xF8),
    tertiary: Color::from_rgb8(0xEF, 0xB8, 0xC8),
    tertiary_container: Color::from_rgb8(0x63, 0x3B, 0x48),
    on_tertiary_container: Color::from_rgb8(0xFF, 0xD8, 0xE4),
    error: Color::from_rgb8(0xF2, 0xB8, 0xB5),
    on_error: Color::from_rgb8(0x60, 0x14, 0x10),
    error_container: Color::from_rgb8(0x8C, 0x1D, 0x18),
    on_error_container: Color::from_rgb8(0xF9, 0xDE, 0xDC),
    surface: Color::from_rgb8(0x14, 0x12, 0x18),
    on_surface: Color::from_rgb8(0xE6, 0xE0, 0xE9),
    on_surface_variant: Color::from_rgb8(0xCA, 0xC4, 0xD0),
    surface_container: Color::from_rgb8(0x21, 0x1F, 0x26),
    surface_container_low: Color::from_rgb8(0x1D, 0x1B, 0x20),
    surface_container_highest: Color::from_rgb8(0x36, 0x34, 0x3B),
    outline: Color::from_rgb8(0x93, 0x8F, 0x99),
    outline_variant: Color::from_rgb8(0x49, 0x45, 0x4F),
    inverse_surface: Color::from_rgb8(0xE6, 0xE0, 0xE9),
    inverse_on_surface: Color::from_rgb8(0x32, 0x2F, 0x35),
    success_container: Color::from_rgb8(0x2E, 0x7D, 0x32),
    on_success_container: Color::from_rgb8(0xC8, 0xE6, 0xC9),
};

/// MD3 "state layer" opacity for a hovered interactive component: hover is
/// communicated by overlaying a translucent wash of the foreground color
/// over the resting fill, not by swapping to a new fill color.
pub const HOVER_STATE_OPACITY: f32 = 0.08;
/// MD3 "state layer" opacity for a pressed (or menu-open) interactive
/// component -- see `HOVER_STATE_OPACITY`.
pub const PRESSED_STATE_OPACITY: f32 = 0.10;
/// MD3's disabled-container opacity: a disabled component's container (and
/// its border, if any) is `on_surface` at this opacity.
pub const DISABLED_CONTAINER_OPACITY: f32 = 0.12;
/// MD3's disabled-content opacity: a disabled component's text/icon is
/// `on_surface` at this opacity.
pub const DISABLED_CONTENT_OPACITY: f32 = 0.38;

/// Linearly interpolates every channel (including alpha) from `from` to
/// `to` at `t` (`0.0` = `from`, `1.0` = `to`). The shared primitive behind
/// `state_layer` and the Flow Bar's pulse/fade animations
/// (`crate::flow_bar`).
pub fn lerp(from: Color, to: Color, t: f32) -> Color {
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

/// MD3's state-layer wash: overlays `on_color` onto `base` at `opacity`,
/// keeping `base`'s own alpha (a state layer tints a fill, it doesn't fade
/// it). Use `HOVER_STATE_OPACITY`/`PRESSED_STATE_OPACITY` for `opacity`.
pub fn state_layer(base: Color, on_color: Color, opacity: f32) -> Color {
    Color {
        a: base.a,
        ..lerp(base, on_color, opacity)
    }
}

/// A translucent wash of `on_color` alone, for components with no resting
/// fill to layer onto (outlined/text buttons -- see
/// `styles::button::outlined`/`styles::button::text`) or for dimming a
/// color by a flat opacity (MD3's disabled-state treatment).
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
