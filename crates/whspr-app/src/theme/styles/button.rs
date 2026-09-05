//! Button styles: `filled` (primary actions), `tonal` (secondary
//! emphasis), `outlined` (repeated per-row actions), and `text` (low-
//! emphasis utility actions). MD3 maps every button variant to the `full`
//! shape token and communicates hover/press via a state-layer wash rather
//! than a new fill color -- see `crate::theme::color::state_layer`.

use iced::widget::button::{Status, Style};
use iced::{Background, Border, Color};

use crate::theme::{color, shape};

/// A primary, high-emphasis action ("Diarize a recording...").
pub fn filled(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: Some(Background::Color(scheme.primary)),
        text_color: scheme.on_primary,
        border: Border::default().rounded(shape::FULL),
        ..Style::default()
    };

    styled(base, scheme.primary, scheme.on_primary, scheme, status)
}

/// A complementary, tonal-emphasis action ("Preview a new hotkey").
pub fn tonal(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: Some(Background::Color(scheme.secondary_container)),
        text_color: scheme.on_secondary_container,
        border: Border::default().rounded(shape::FULL),
        ..Style::default()
    };

    styled(
        base,
        scheme.secondary_container,
        scheme.on_secondary_container,
        scheme,
        status,
    )
}

/// A repeated, low-emphasis action against a card background (a speaker
/// row's "Save"): outlined instead of filled so a whole list of rows
/// doesn't turn into a wall of filled buttons.
pub fn outlined(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: None,
        text_color: scheme.primary,
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: shape::FULL.into(),
        },
        ..Style::default()
    };

    washed(base, scheme.primary, scheme, status)
}

/// A text-only utility action (the Hub header's theme toggle).
pub fn text(scheme: &color::Scheme, status: Status) -> Style {
    let base = Style {
        background: None,
        text_color: scheme.primary,
        ..Style::default()
    };

    washed(base, scheme.primary, scheme, status)
}

/// Applies MD3's state-layer wash on top of a filled `base` style.
fn styled(base: Style, fill: Color, on_fill: Color, scheme: &color::Scheme, status: Status) -> Style {
    match status {
        Status::Active => base,
        Status::Hovered => Style {
            background: Some(Background::Color(color::state_layer(
                fill,
                on_fill,
                color::HOVER_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Pressed => Style {
            background: Some(Background::Color(color::state_layer(
                fill,
                on_fill,
                color::PRESSED_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Disabled => disabled(scheme, base),
    }
}

/// Applies MD3's state-layer wash on top of a transparent-background
/// `base` style (outlined/text buttons), where there's no fill to tint --
/// just a translucent wash of the content color itself.
fn washed(base: Style, content: Color, scheme: &color::Scheme, status: Status) -> Style {
    match status {
        Status::Active => base,
        Status::Hovered => Style {
            background: Some(Background::Color(color::wash(
                content,
                color::HOVER_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Pressed => Style {
            background: Some(Background::Color(color::wash(
                content,
                color::PRESSED_STATE_OPACITY,
            ))),
            ..base
        },
        Status::Disabled => disabled(scheme, base),
    }
}

/// MD3's disabled treatment: `on_surface` at low opacity for the
/// container, its border (if any), and its content -- applied from the
/// scheme's neutral role rather than each variant's own colors, per spec.
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
            Background::Color(color::wash(scheme.on_surface, color::DISABLED_CONTAINER_OPACITY))
        }),
        text_color: color::wash(scheme.on_surface, color::DISABLED_CONTENT_OPACITY),
        border,
        ..base
    }
}
