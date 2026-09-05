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
