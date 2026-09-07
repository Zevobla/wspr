//! Modernist's spacing scale (`--space-1..8` = 4/8/12/16/24/32) plus the
//! fixed structural metrics the shell is built on (`layout`). Every
//! `spacing`/`padding` call in `hub`/`flow_bar` reaches for one of these
//! rather than a bare number, so the grid stays a single source of truth.

/// A tight pairing, e.g. a field caption sitting right above its control.
pub const XS: f32 = 4.0;
/// A related group, e.g. an icon next to its label inside a button.
pub const SM: f32 = 8.0;
/// Between fields within the same section.
pub const MD: f32 = 12.0;
/// A section's internal padding / gap between rows.
pub const LG: f32 = 16.0;
/// Between sections.
pub const XL: f32 = 24.0;
/// The window inset and the widest structural gap (`--space-8`).
pub const XXL: f32 = 32.0;

/// Fixed structural metrics for the Modernist shell -- the numbered nav
/// rail, the screen header band, and the Settings three-column layout.
/// These are architectural constants from the System sheet (mockup 1b),
/// not part of the 4px content-spacing scale above.
pub mod layout {
    /// Width of the left numbered nav rail.
    pub const RAIL_W: f32 = 208.0;
    /// Width of the Settings screen's middle sub-nav column.
    pub const SETTINGS_SUBNAV_W: f32 = 176.0;
    /// Height of a screen's header band (title + trailing actions).
    pub const HEADER_H: f32 = 72.0;
    /// Height of the nav rail's brand block, matched to `HEADER_H` so the
    /// rail's top rule lines up with the screen header's bottom rule.
    pub const RAIL_HEADER_H: f32 = 72.0;
    /// Bottom padding shared by the screen header's title
    /// (`crate::theme::widgets::screen_header`) and the rail's brand block
    /// (`crate::hub::brand`) -- both bottom-align their content in their
    /// `HEADER_H`/`RAIL_HEADER_H` band with this same padding, so "Dictate"
    /// and "whspr" land on the same line regardless of the two bands'
    /// differently-sized content (a 28px title vs. a 12px mark + 18px word).
    pub const HEADER_TITLE_PAD_BOTTOM: f32 = 7.5;
    /// The system's strong divider weight -- 2px rules between major
    /// sections (`crate::theme::styles::container::divider`).
    pub const RULE: f32 = 2.0;
    /// The system's hairline weight -- 1px rules between table rows and
    /// sub-nav columns.
    pub const HAIRLINE: f32 = 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_values_increase_monotonically() {
        let steps = [XS, SM, MD, LG, XL, XXL];
        for pair in steps.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    #[test]
    fn spacing_values_follow_the_modernist_scale() {
        assert_eq!(
            [XS, SM, MD, LG, XL, XXL],
            [4.0, 8.0, 12.0, 16.0, 24.0, 32.0]
        );
    }
}
