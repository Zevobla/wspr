//! Container styles for the Hub's chrome (background, section panels,
//! header divider, error banner) and the Flow Bar overlay.

use iced::widget::container::Style;
use iced::{Background, Border, Color, Shadow, Vector};

use crate::theme::{color, shape};

/// The Hub window's outer background.
pub fn surface(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.surface)),
        text_color: Some(scheme.on_surface),
        ..Style::default()
    }
}

/// A settings-panel "card": MD3 elevation communicated through tonal color
/// (`surface_container_low`) rather than a shadow -- see the material-3
/// skill's elevation notes on why shadows aren't the default depth cue.
pub fn section(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.surface_container_low)),
        text_color: Some(scheme.on_surface),
        border: Border::default().rounded(shape::MD),
        ..Style::default()
    }
}

/// A 1px `outline_variant` rule, e.g. under the Hub's header.
pub fn divider(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.outline_variant)),
        ..Style::default()
    }
}

/// The error banner: styled as an MD3 snackbar (`inverse_surface`,
/// extra-small shape) since it's the same kind of thing -- a transient,
/// high-contrast notice laid over the rest of the UI.
pub fn error_banner(scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(scheme.inverse_surface)),
        text_color: Some(scheme.inverse_on_surface),
        border: Border::default().rounded(shape::XS),
        ..Style::default()
    }
}

/// The Flow Bar overlay's pill. `fill` is the current (possibly animated)
/// container color from `crate::flow_bar::animate`; a thin
/// `outline_variant` border plus a soft shadow keep the pill legible over
/// arbitrary desktop content behind it -- the one place in this app a
/// shadow earns its keep, since the pill genuinely floats over content
/// that could be anything (see the material-3 skill's elevation notes on
/// when shadows are appropriate).
pub fn flow_bar(fill: Color, scheme: &color::Scheme) -> Style {
    Style {
        background: Some(Background::Color(fill)),
        border: Border {
            color: scheme.outline_variant,
            width: 1.0,
            radius: shape::LG.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..Style::default()
    }
}
