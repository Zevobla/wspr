//! Modernist button styles. The public function names are carried over
//! from the previous scheme so call sites compile unchanged, but each now
//! realizes a Modernist variant (design guide `.btn-*`):
//!
//! - `filled`   -> `.btn-primary`: solid accent, paper text; hover/press
//!   step the accent one/two ramp steps (never a translucent wash).
//! - `outlined` -> `.btn-secondary`: transparent with a 1px divider border,
//!   ink text; hover/press are a faint ink wash.
//! - `text`     -> `.btn-ghost`: accent text, no border; hover/press a faint
//!   accent wash.
//! - `tonal`    -> a neutral tinted fill (segmented/inactive emphasis).
//! - `error`    -> the destructive/active accent fill (Dictate's "Stop").
//!
//! Every variant has zero radius (`shape::NONE`); labels are laid out
//! flush-left by the caller (see `crate::theme::widgets`), not centered.

use iced::widget::button::{Status, Style};
use iced::{Background, Border, Color};

use crate::theme::{color, shape};

const GHOST_HOVER: f32 = 0.10;
const GHOST_PRESSED: f32 = 0.18;
const SECONDARY_HOVER: f32 = 0.07;
const SECONDARY_PRESSED: f32 = 0.14;

/// `.btn-primary`: a solid accent fill (the one primary action per screen).
pub fn filled(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: Some(Background::Color(scheme.primary)),
        text_color: scheme.on_primary,
        border: Border::default().rounded(shape::NONE),
        ..Style::default()
    };
    match status {
        Status::Active => base,
        Status::Hovered => Style {
            background: Some(Background::Color(scheme.accent_hover)),
            ..base
        },
        Status::Pressed => Style {
            background: Some(Background::Color(scheme.accent_pressed)),
            ..base
        },
        Status::Disabled => disabled(scheme, base),
    }
}

/// A neutral tinted fill for secondary emphasis where a border would read
/// too quietly (segmented-control active option shares this look).
pub fn tonal(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: Some(Background::Color(scheme.secondary_container)),
        text_color: scheme.on_secondary_container,
        border: Border::default().rounded(shape::NONE),
        ..Style::default()
    };
    match status {
        Status::Active => base,
        Status::Hovered => Style {
            background: Some(Background::Color(color::state_layer(
                scheme.secondary_container,
                scheme.on_secondary_container,
                color::HOVER_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Pressed => Style {
            background: Some(Background::Color(color::state_layer(
                scheme.secondary_container,
                scheme.on_secondary_container,
                color::PRESSED_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Disabled => disabled(scheme, base),
    }
}

/// `.btn-secondary`: transparent with a 1px divider border and ink text.
pub fn outlined(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: shape::NONE.into(),
        },
        ..Style::default()
    };
    match status {
        Status::Active => base,
        Status::Hovered => washed(base, scheme.on_surface, SECONDARY_HOVER),
        Status::Pressed => washed(base, scheme.on_surface, SECONDARY_PRESSED),
        Status::Disabled => disabled(scheme, base),
    }
}

/// `.btn-ghost`: accent text, no border, a faint accent wash on hover.
pub fn text(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: None,
        text_color: scheme.primary,
        ..Style::default()
    };
    match status {
        Status::Active => base,
        Status::Hovered => washed(base, scheme.primary, GHOST_HOVER),
        Status::Pressed => washed(base, scheme.primary, GHOST_PRESSED),
        Status::Disabled => disabled(scheme, base),
    }
}

/// Lays a translucent wash of `content` over a transparent `base`.
fn washed(base: Style, content: Color, opacity: f32) -> Style {
    Style {
        background: Some(Background::Color(color::wash(content, opacity))),
        ..base
    }
}

/// The disabled treatment: fill/border/content drop to `on_surface` at a
/// low opacity, per the scheme's neutral disabled roles.
fn disabled(scheme: &color::Scheme, base: Style) -> Style {
    let border = if base.border.width > 0.0 {
        Border {
            color: color::wash(scheme.on_surface, color::DISABLED_CONTAINER_OPACITY),
            ..base.border
        }
    } else {
        base.border
    };
    Style {
        background: base.background.map(|_| {
            Background::Color(color::wash(
                scheme.on_surface,
                color::DISABLED_CONTAINER_OPACITY,
            ))
        }),
        text_color: color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY),
        border,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_button_has_accent_background() {
        let scheme = &color::LIGHT;
        let style = filled(scheme, Status::Active);
        assert_eq!(style.background, Some(Background::Color(scheme.primary)));
        assert_eq!(style.text_color, scheme.on_primary);
    }

    #[test]
    fn filled_button_darkens_on_hover_and_press() {
        let scheme = &color::LIGHT;
        assert_eq!(
            filled(scheme, Status::Hovered).background,
            Some(Background::Color(scheme.accent_hover))
        );
        assert_eq!(
            filled(scheme, Status::Pressed).background,
            Some(Background::Color(scheme.accent_pressed))
        );
    }

    #[test]
    fn tonal_button_has_secondary_container_background() {
        let scheme = &color::LIGHT;
        let style = tonal(scheme, Status::Active);
        assert!(style.background.is_some());
        assert_eq!(style.text_color, scheme.on_secondary_container);
    }

    #[test]
    fn secondary_button_is_transparent_with_ink_text_and_a_border() {
        let scheme = &color::LIGHT;
        let style = outlined(scheme, Status::Active);
        assert!(style.background.is_none());
        assert_eq!(style.text_color, scheme.on_surface);
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.color, scheme.outline);
    }

    #[test]
    fn ghost_button_has_accent_text_and_no_background() {
        let scheme = &color::LIGHT;
        let style = text(scheme, Status::Active);
        assert!(style.background.is_none());
        assert_eq!(style.text_color, scheme.primary);
    }

    #[test]
    fn every_button_variant_is_square() {
        let scheme = &color::LIGHT;
        for style in [
            filled(scheme, Status::Active),
            tonal(scheme, Status::Active),
            outlined(scheme, Status::Active),
        ] {
            assert_eq!(style.border.radius, shape::NONE.into());
        }
    }

    #[test]
    fn filled_button_disabled_dims_content() {
        let scheme = &color::LIGHT;
        let style = filled(scheme, Status::Disabled);
        assert_eq!(
            style.text_color,
            color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY)
        );
    }
}
