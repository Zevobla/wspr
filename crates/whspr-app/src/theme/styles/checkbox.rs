//! Checkbox style: MD3's selection-control color mapping -- an outlined
//! box when unchecked, a `primary`-filled box with an `on_primary` mark
//! when checked -- for the Hub's "Launch at login" toggle.

use iced::widget::checkbox::{Status, Style};
use iced::{Background, Border, Color};

use crate::theme::{color, shape};

pub fn field(scheme: &color::Scheme, status: Status) -> Style {
    let is_checked = match status {
        Status::Active { is_checked }
        | Status::Hovered { is_checked }
        | Status::Disabled { is_checked } => is_checked,
    };

    let base = Style {
        background: if is_checked {
            Background::Color(scheme.primary)
        } else {
            Background::Color(Color::TRANSPARENT)
        },
        icon_color: scheme.on_primary,
        border: Border {
            color: if is_checked {
                scheme.primary
            } else {
                scheme.outline
            },
            width: 2.0,
            radius: shape::XS.into(),
        },
        text_color: Some(scheme.on_surface),
    };

    match status {
        Status::Active { .. } => base,
        Status::Hovered { .. } => Style {
            border: Border {
                color: if is_checked {
                    scheme.primary
                } else {
                    scheme.on_surface
                },
                ..base.border
            },
            ..base
        },
        Status::Disabled { .. } => Style {
            background: if is_checked {
                Background::Color(color::wash(
                    scheme.on_surface,
                    color::DISABLED_CONTAINER_OPACITY,
                ))
            } else {
                Background::Color(Color::TRANSPARENT)
            },
            icon_color: color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY),
            border: Border {
                color: color::wash(scheme.on_surface, color::DISABLED_CONTAINER_OPACITY),
                ..base.border
            },
            text_color: Some(color::wash(
                scheme.on_surface,
                color::DISABLED_CONTENT_OPACITY,
            )),
        },
    }
}
