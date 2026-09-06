//! MD3's 4dp spacing grid, trimmed to the steps whspr-app's layouts use.
//! Every `spacing`/`padding` call in `hub`/`flow_bar` reaches for one of
//! these rather than a bare number, so the grid stays a single source of
//! truth instead of driftable magic numbers scattered per call site.

/// A tight pairing, e.g. a field caption sitting right above its control.
pub const XS: f32 = 4.0;
/// A related group, e.g. a status line next to the button that produced it.
pub const SM: f32 = 8.0;
/// Between fields within the same section.
pub const MD: f32 = 12.0;
/// A section panel's internal padding.
pub const LG: f32 = 16.0;
/// Between sections, and the Hub/Flow Bar's outer padding.
pub const XL: f32 = 24.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_values_increase_monotonically() {
        assert!(XS < SM);
        assert!(SM < MD);
        assert!(MD < LG);
        assert!(LG < XL);
    }

    #[test]
    fn spacing_values_follow_4dp_grid() {
        assert_eq!(XS, 4.0);
        assert_eq!(SM, 8.0);
        assert_eq!(MD, 12.0);
        assert_eq!(LG, 16.0);
        assert_eq!(XL, 24.0);
    }
}
