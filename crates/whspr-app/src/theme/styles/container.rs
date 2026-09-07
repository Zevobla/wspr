//! Modernist container styles: the Hub chrome (paper ground, tinted
//! sections/cards, the nav rail), the divider rules, the tag fills, the
//! error notice, and the Flow Bar overlay. Nothing inside the Hub is
//! shadowed -- structure is carried by 2px rules, not elevation -- so the
//! only shadow lives on the Flow Bar, which genuinely floats over arbitrary
//! desktop content.

use iced::widget::container::Style;
use iced::{Background, Border, Color, Shadow, Vector};

use crate::theme::{color, shape};

/// The Flow Bar's ambient shadow (`--shadow-md`): a soft ink-tinted drop.
const FLOW_BAR_SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.16),
    offset: Vector::new(0.0, 3.0),
    blur_radius: 10.0,
};

/// The Hub window's ground (paper).
pub fn surface(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.surface)),
        text_color: Some(scheme.on_surface),
        ..Style::default()
    }
}

/// The left numbered nav rail: the same paper ground as the window; a 2px
/// right rule (drawn as a sibling by `crate::hub`) separates it, not a fill.
pub fn rail(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.surface)),
        text_color: Some(scheme.on_surface),
        ..Style::default()
    }
}

/// A section panel / card: the one tinted fill, zero radius, no shadow.
pub fn section(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.surface_container_low)),
        text_color: Some(scheme.on_surface),
        border: Border::default().rounded(shape::NONE),
        ..Style::default()
    }
}

/// A content card -- same tinted fill as `section`; named for call-site
/// clarity where the element is a card rather than a settings panel.
pub fn card(scheme: &color::Scheme) -> Style {
    section(scheme)
}

/// A divider rule (ink @ 40%). The caller sets the weight via the
/// container's height: 2px for strong section rules, 1px for hairlines.
pub fn divider(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.outline_variant)),
        ..Style::default()
    }
}

/// A solid ink fill (rail active-mark squares, status dots when inverted).
pub fn ink(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.on_surface)),
        ..Style::default()
    }
}

/// A solid accent fill (active rail mark, recording square, meter bars).
pub fn accent(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.primary)),
        ..Style::default()
    }
}

/// `.tag-accent`: accent-100 tint with accent-800 text.
pub fn tag_accent(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.accent_ramp[0])),
        text_color: Some(scheme.accent_ramp[7]),
        border: Border::default().rounded(shape::NONE),
        ..Style::default()
    }
}

/// `.tag-neutral`: neutral-100 tint with neutral-800 text.
pub fn tag_neutral(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.neutral[0])),
        text_color: Some(scheme.neutral[7]),
        border: Border::default().rounded(shape::NONE),
        ..Style::default()
    }
}

/// `.tag-outline`: transparent with a 1px accent border and accent text.
pub fn tag_outline(scheme: &color::Scheme) -> Style {
    Style {
        background: None,
        text_color: Some(scheme.primary),
        border: Border {
            color: scheme.primary,
            width: 1.0,
            radius: shape::NONE.into(),
        },
        ..Style::default()
    }
}

/// The error notice: an accent-100 tint with accent-800 text and a 1px
/// accent border -- Modernist keeps errors mono (red), never a dark
/// snackbar.
pub fn error_banner(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.error_container)),
        text_color: Some(scheme.on_error_container),
        border: Border {
            color: scheme.primary,
            width: 1.0,
            radius: shape::NONE.into(),
        },
        ..Style::default()
    }
}

/// The Flow Bar overlay: a flat rectangle (radius 0) with a 1px divider
/// hairline and one soft ambient shadow so it stays legible floating over
/// arbitrary content. `fill` is the current (possibly animated) color from
/// `crate::flow_bar::animate`.
pub fn flow_bar(fill: Color, scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(fill)),
        border: Border {
            color: scheme.outline_variant,
            width: 1.0,
            radius: shape::NONE.into(),
        },
        shadow: FLOW_BAR_SHADOW,
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_has_paper_background_and_ink_text() {
        let scheme = &color::LIGHT;
        let style = surface(scheme);
        assert_eq!(style.background, Some(Background::Color(scheme.surface)));
        assert_eq!(style.text_color, Some(scheme.on_surface));
    }

    #[test]
    fn section_is_a_square_tinted_fill() {
        let scheme = &color::LIGHT;
        let style = section(scheme);
        assert!(style.background.is_some());
        assert_eq!(style.border.radius, shape::NONE.into());
    }

    #[test]
    fn tags_carry_their_ramp_colors() {
        let scheme = &color::LIGHT;
        assert_eq!(
            tag_accent(scheme).background,
            Some(Background::Color(scheme.accent_ramp[0]))
        );
        assert_eq!(tag_outline(scheme).background, None);
        assert_eq!(tag_outline(scheme).border.color, scheme.primary);
    }

    #[test]
    fn error_banner_is_a_mono_accent_notice() {
        let scheme = &color::LIGHT;
        let style = error_banner(scheme);
        assert_eq!(style.text_color, Some(scheme.on_error_container));
        assert_eq!(style.border.color, scheme.primary);
    }

    #[test]
    fn flow_bar_is_square_and_shadowed() {
        let scheme = &color::LIGHT;
        let style = flow_bar(scheme.primary, scheme);
        assert_eq!(style.border.radius, shape::NONE.into());
        assert!(style.shadow.blur_radius > 0.0);
    }
}
