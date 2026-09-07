//! Slider style: MD3's mapping for a continuous-value control -- a
//! `primary`-filled rail up to the handle, `surface_container_highest` for
//! the remainder, and a circular `primary` handle that grows slightly on
//! hover/drag. Used by the Settings screen's Capture section (input gain,
//! voice-activity threshold).

use iced::widget::slider::{Handle, HandleShape, Rail, Status, Style};
use iced::{Background, Border};

use crate::theme::color;

pub fn field(scheme: &color::Scheme, status: Status) -> Style {
    let active = Style {
        rail: Rail {
            backgrounds: (
                Background::Color(scheme.primary),
                Background::Color(scheme.surface_container_highest),
            ),
            width: 4.0,
            border: Border {
                color: scheme.outline_variant,
                width: 0.0,
                radius: 2.0.into(),
            },
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 8.0 },
            background: Background::Color(scheme.primary),
            border_width: 0.0,
            border_color: scheme.primary,
        },
    };

    match status {
        Status::Active => active,
        Status::Hovered | Status::Dragged => Style {
            handle: Handle {
                shape: HandleShape::Circle { radius: 9.0 },
                ..active.handle
            },
            ..active
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_slider_fills_rail_with_primary() {
        let scheme = &color::LIGHT;
        let style = field(scheme, Status::Active);
        assert_eq!(style.rail.backgrounds.0, Background::Color(scheme.primary));
        assert_eq!(style.handle.background, Background::Color(scheme.primary));
    }

    #[test]
    fn hovered_slider_grows_the_handle() {
        let scheme = &color::LIGHT;
        let active = field(scheme, Status::Active);
        let hovered = field(scheme, Status::Hovered);
        assert_ne!(active.handle.shape, hovered.handle.shape);
    }

    #[test]
    fn dragged_slider_matches_hovered_handle_size() {
        let scheme = &color::LIGHT;
        let hovered = field(scheme, Status::Hovered);
        let dragged = field(scheme, Status::Dragged);
        assert_eq!(hovered.handle.shape, dragged.handle.shape);
    }
}
