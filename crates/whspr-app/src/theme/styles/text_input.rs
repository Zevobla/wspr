//! Text field style: MD3's "outlined" text field, used for the Speakers
//! section's rename input.

use iced::widget::text_input::{Status, Style};
use iced::{Background, Border};

use crate::theme::{color, shape};

pub fn outlined(scheme: &color::Scheme, status: Status) -> Style {
    let active = Style {
        background: Background::Color(scheme.surface_container_highest),
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: shape::SM.into(),
        },
        icon: scheme.on_surface_variant,
        placeholder: scheme.on_surface_variant,
        value: scheme.on_surface,
        selection: scheme.primary_container,
    };

    match status {
        Status::Active => active,
        Status::Hovered => Style {
            border: Border {
                color: scheme.on_surface,
                ..active.border
            },
            ..active
        },
        Status::Focused { .. } => Style {
            border: Border {
                color: scheme.primary,
                width: 2.0,
                ..active.border
            },
            ..active
        },
        Status::Disabled => Style {
            background: Background::Color(color::wash(
                scheme.on_surface,
                color::DISABLED_CONTAINER_OPACITY,
            )),
            value: color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY),
            placeholder: color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY),
            border: Border {
                color: color::wash(scheme.on_surface, color::DISABLED_CONTAINER_OPACITY),
                ..active.border
            },
            ..active
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_text_input_has_outline_border() {
        let scheme = &color::LIGHT;
        let style = outlined(scheme, Status::Active);
        assert_eq!(style.border.color, scheme.outline);
        assert_eq!(style.value, scheme.on_surface);
    }

    #[test]
    fn hovered_text_input_changes_border_color() {
        let scheme = &color::LIGHT;
        let active_style = outlined(scheme, Status::Active);
        let hovered_style = outlined(scheme, Status::Hovered);
        assert_ne!(active_style.border.color, hovered_style.border.color);
    }

    #[test]
    fn focused_text_input_uses_primary_and_wider_border() {
        let scheme = &color::LIGHT;
        let style = outlined(scheme, Status::Focused { is_focused: true });
        assert_eq!(style.border.color, scheme.primary);
        assert_eq!(style.border.width, 2.0);
    }

    #[test]
    fn disabled_text_input_uses_on_surface_color() {
        let scheme = &color::LIGHT;
        let style = outlined(scheme, Status::Disabled);
        assert_eq!(style.value, color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY));
    }
}
