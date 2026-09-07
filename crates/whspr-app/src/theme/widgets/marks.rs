//! The Modernist geometric status marks and the level meter -- drawn from
//! `container` + `Border` primitives (never icons) so they stay crisp and
//! theme-reactive at any DPI. Shared by the nav rail, the Dictate hero, the
//! Flow Bar and the tray parity glyphs.

use iced::widget::{container, row, Space};
use iced::{Alignment, Border, Element, Length};

use crate::theme::{color, styles};

/// A status glyph shape. The mark *shape* (not just color) changes with
/// state, so it stays legible in a monochrome menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Idle: a hollow 2px-outlined square.
    Outline,
    /// Active/recording: a solid accent square.
    Solid,
    /// Thinking: a square half-filled with accent.
    Half,
    /// A solid ink square (status line bullet).
    Ink,
}

/// A `size`x`size` status square in the given `kind`.
pub fn status_square<'a, M: 'a>(
    kind: Mark,
    size: f32,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let square = container(Space::new())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));
    match kind {
        Mark::Solid => square
            .style(move |_theme| styles::container::accent(scheme))
            .into(),
        Mark::Ink => square
            .style(move |_theme| styles::container::ink(scheme))
            .into(),
        Mark::Outline => square
            .style(move |_theme| container::Style {
                border: Border {
                    color: scheme.on_surface,
                    width: 2.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into(),
        Mark::Half => container(
            row![
                container(Space::new())
                    .width(Length::Fixed(size / 2.0))
                    .height(Length::Fill)
                    .style(move |_theme| styles::container::accent(scheme)),
                Space::new().width(Length::Fill),
            ]
            .height(Length::Fill),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme| container::Style {
            border: Border {
                color: scheme.on_surface,
                width: 2.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into(),
    }
}

/// A live level meter: `bars` thin vertical accent bars whose heights scale
/// with `level` (0.0..=1.0) off a small resting floor, so an idle meter
/// still shows structure. Bottom-aligned inside a `height`-tall row.
pub fn meter<'a, M: 'a>(
    level: f32,
    bars: usize,
    height: f32,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    // A fixed profile so adjacent bars differ in height (a flat block reads
    // as a progress bar, not a meter); scaled by the live level.
    const PROFILE: [f32; 7] = [0.55, 0.85, 0.4, 1.0, 0.65, 0.9, 0.5];
    let level = level.clamp(0.0, 1.0);
    let bar_els = (0..bars).map(|i| {
        let shape = PROFILE[i % PROFILE.len()];
        let h = height * (0.14 + level * shape * 0.86);
        container(Space::new())
            .width(Length::Fixed(3.0))
            .height(Length::Fixed(h.max(2.0)))
            .style(move |_theme| styles::container::accent(scheme))
            .into()
    });
    container(
        row(bar_els)
            .spacing(3)
            .align_y(Alignment::End)
            .height(Length::Fixed(height)),
    )
    .height(Length::Fixed(height))
    .align_y(Alignment::End)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_distinct_variants() {
        assert_ne!(Mark::Outline, Mark::Solid);
        assert_ne!(Mark::Half, Mark::Ink);
    }

    #[test]
    fn meter_level_is_clamped() {
        // Smoke test: building a meter with an out-of-range level must not
        // panic (heights are clamped/floored internally).
        let _: Element<'_, ()> = meter(5.0, 12, 48.0, &color::LIGHT);
        let _: Element<'_, ()> = meter(-1.0, 12, 48.0, &color::LIGHT);
    }
}
