//! Pick list styles: the closed field (`field`, MD3 "outlined" like the
//! text field) and its dropdown menu (`menu`, an MD3 elevation-2 surface).

use iced::widget::overlay::menu;
use iced::widget::pick_list::{Status, Style};
use iced::{Background, Border, Shadow, Vector};

use crate::theme::{color, shape};

pub fn field(scheme: &color::Scheme, status: Status) -> Style {
    let active = Style {
        text_color: scheme.on_surface,
        placeholder_color: scheme.on_surface_variant,
        handle_color: scheme.on_surface_variant,
        background: Background::Color(scheme.surface_container_highest),
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: shape::SM.into(),
        },
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
        Status::Opened { .. } => Style {
            background: Background::Color(color::state_layer(
                scheme.surface_container_highest,
                scheme.primary,
                color::HOVER_STATE_OPACITY,
            )),
            border: Border {
                color: scheme.primary,
                width: 2.0,
                ..active.border
            },
            ..active
        },
    }
}

pub fn menu(scheme: &color::Scheme) -> menu::Style {
    menu::Style {
        background: Background::Color(scheme.surface_container),
        border: Border {
            color: scheme.outline_variant,
            width: 1.0,
            radius: shape::SM.into(),
        },
        text_color: scheme.on_surface,
        selected_text_color: scheme.on_secondary_container,
        selected_background: Background::Color(scheme.secondary_container),
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_pick_list_has_correct_background() {
        let scheme = &color::LIGHT;
        let style = field(scheme, Status::Active);
        assert_eq!(style.text_color, scheme.on_surface);
        assert_eq!(style.handle_color, scheme.on_surface_variant);
    }

    #[test]
    fn hovered_pick_list_changes_border_color() {
        let scheme = &color::LIGHT;
        let active_style = field(scheme, Status::Active);
        let hovered_style = field(scheme, Status::Hovered);
        assert_ne!(active_style.border.color, hovered_style.border.color);
    }

    #[test]
    fn opened_pick_list_increases_border_width() {
        let scheme = &color::LIGHT;
        let style = field(scheme, Status::Opened { with_default: false });
        assert_eq!(style.border.width, 2.0);
    }

    #[test]
    fn menu_has_secondary_container_selection() {
        let scheme = &color::LIGHT;
        let style = menu(scheme);
        assert_eq!(style.text_color, scheme.on_surface);
        assert_eq!(style.selected_text_color, scheme.on_secondary_container);
    }

    #[test]
    fn menu_has_shadow() {
        let scheme = &color::LIGHT;
        let style = menu(scheme);
        assert_eq!(style.shadow.blur_radius, 6.0);
    }
}
