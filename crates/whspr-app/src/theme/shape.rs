//! MD3 shape tokens: the corner-radius scale, from a subtle round to a
//! full pill. See the MD3 skill's component-shape-mapping table for which
//! token each component defaults to; `styles` reaches for these instead of
//! hardcoding a radius inline.

/// Snackbars -- see `styles::container::error_banner`.
pub const XS: f32 = 4.0;
/// Text fields, pick lists, and their dropdown menus.
pub const SM: f32 = 8.0;
/// Cards -- the Hub's section panels.
pub const MD: f32 = 12.0;
/// FAB-scale elements -- the Flow Bar overlay.
pub const LG: f32 = 16.0;
/// Buttons and scrollbar thumbs: fully rounded regardless of height.
pub const FULL: f32 = 9999.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_tokens_have_expected_values() {
        assert_eq!(XS, 4.0);
        assert_eq!(SM, 8.0);
        assert_eq!(MD, 12.0);
        assert_eq!(LG, 16.0);
        assert_eq!(FULL, 9999.0);
    }

    #[test]
    fn shape_tokens_scale_up_progressively() {
        assert!(XS < SM);
        assert!(SM < MD);
        assert!(MD < LG);
        assert!(LG < FULL);
    }
}
