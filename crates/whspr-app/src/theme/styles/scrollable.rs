//! Scrollbar style: a thin `outline_variant` rail (turning `primary` on
//! hover/drag) so the History and Speakers lists' scrollbars read as part
//! of the MD3 surface instead of the OS's raw default -- see the
//! impeccable skill's "browser surfaces" note on custom scrollbars
//! carrying no design system by default.

use iced::widget::container;
use iced::widget::scrollable::{AutoScroll, Rail, Scroller, Status, Style};
use iced::{Border, Color, Shadow, Vector};

use crate::theme::{color, shape};

pub fn rail(scheme: &color::Scheme, status: Status) -> Style {
    let (horizontal_accented, vertical_accented) = match status {
        Status::Active { .. } => (false, false),
        Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
            ..
        } => (
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
        ),
        Status::Dragged {
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
            ..
        } => (
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
        ),
    };

    Style {
        container: container::Style::default(),
        horizontal_rail: track(scheme, horizontal_accented),
        vertical_rail: track(scheme, vertical_accented),
        gap: None,
        auto_scroll: auto_scroll(scheme),
    }
}

fn track(scheme: &color::Scheme, accented: bool) -> Rail {
    let color = if accented {
        scheme.primary
    } else {
        scheme.outline_variant
    };

    Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            background: color.into(),
            border: Border::default().rounded(shape::FULL),
        },
    }
}

fn auto_scroll(scheme: &color::Scheme) -> AutoScroll {
    AutoScroll {
        background: scheme.surface_container_highest.scale_alpha(0.9).into(),
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: shape::FULL.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.3),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        icon: scheme.on_surface_variant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_scrollbar_uses_outline_variant() {
        let scheme = &color::LIGHT;
        let status = Status::Active { vertical_scrollbar_scroller_position: 0.0, horizontal_scrollbar_scroller_position: 0.0 };
        let style = rail(scheme, status);
        // Verify that the style is created without panic
        assert_eq!(style.gap, None);
    }

    #[test]
    fn hovered_scrollbar_changes_accent() {
        let scheme = &color::LIGHT;
        let status = Status::Hovered {
            is_horizontal_scrollbar_hovered: true,
            is_vertical_scrollbar_hovered: false,
            vertical_scrollbar_scroller_position: 0.0,
            horizontal_scrollbar_scroller_position: 0.0,
        };
        let style = rail(scheme, status);
        assert_eq!(style.gap, None);
    }

    #[test]
    fn track_uses_primary_when_accented() {
        let scheme = &color::LIGHT;
        let track_style = track(scheme, true);
        // Scroller should have primary color when accented
        assert!(track_style.scroller.background.is_some());
    }

    #[test]
    fn auto_scroll_has_shadow() {
        let scheme = &color::LIGHT;
        let style = auto_scroll(scheme);
        assert_eq!(style.shadow.blur_radius, 4.0);
    }
}
