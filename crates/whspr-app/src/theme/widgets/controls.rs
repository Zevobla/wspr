//! Two custom Modernist form controls iced has no native square version of:
//! the toggle switch (native `toggler` is pill-shaped and can't be squared)
//! and the segmented control (no native widget). Both are built from a
//! `button` + `container` primitives so they honor radius 0.

use iced::widget::{button, container, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::theme::{color, spacing, styles, type_scale};

const TRACK_W: f32 = 32.0;
const TRACK_H: f32 = 18.0;
const KNOB: f32 = 14.0;

/// A square toggle switch. `on_toggle` receives the *new* value. `M: Clone`
/// because the inner `button`'s `Into<Element>` requires it.
pub fn toggle<'a, M: Clone + 'a>(
    is_on: bool,
    on_toggle: impl Fn(bool) -> M + 'a,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let knob = container(Space::new())
        .width(Length::Fixed(KNOB))
        .height(Length::Fixed(KNOB))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(scheme.on_primary)),
            ..container::Style::default()
        });

    let inner = if is_on {
        row![Space::new().width(Length::Fill), knob]
    } else {
        row![knob, Space::new().width(Length::Fill)]
    }
    .align_y(Alignment::Center);

    let track = container(inner)
        .width(Length::Fixed(TRACK_W))
        .height(Length::Fixed(TRACK_H))
        .padding(2)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(if is_on {
                scheme.primary
            } else {
                scheme.neutral[3]
            })),
            border: Border::default().rounded(0.0),
            ..container::Style::default()
        });

    button(track)
        .padding(0)
        .on_press(on_toggle(!is_on))
        .style(|_theme, _status| button::Style {
            background: None,
            text_color: Color::TRANSPARENT,
            ..button::Style::default()
        })
        .into()
}

/// A segmented control: a bordered row of options, exactly one active
/// (accent fill + paper text), 1px dividers between options. `on_select`
/// receives the chosen option's index.
pub fn segmented<'a, M: Clone + 'a>(
    options: Vec<(String, bool)>,
    on_select: impl Fn(usize) -> M + 'a,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let mut r = row![].align_y(Alignment::Center);
    let last = options.len().saturating_sub(1);
    for (i, (label, active)) in options.into_iter().enumerate() {
        let opt = button(
            text(label)
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::LABEL_LARGE.font()),
        )
        .padding([7.0, 12.0])
        .on_press(on_select(i))
        .style(move |_theme, status| segment_style(scheme, active, status));
        r = r.push(opt);
        if i != last {
            r = r.push(
                container(Space::new())
                    .width(Length::Fixed(1.0))
                    .height(Length::Fixed(TRACK_H + 12.0))
                    .style(move |_theme| styles::container::divider(scheme)),
            );
        }
    }

    container(r)
        .style(move |_theme| container::Style {
            border: Border {
                color: scheme.outline,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn segment_style(
    scheme: &'static color::Scheme,
    active: bool,
    status: button::Status,
) -> button::Style {
    if active {
        return button::Style {
            background: Some(Background::Color(scheme.primary)),
            text_color: scheme.on_primary,
            border: Border::default().rounded(0.0),
            ..button::Style::default()
        };
    }
    let base = button::Style {
        background: None,
        text_color: scheme.on_surface,
        border: Border::default().rounded(0.0),
        ..button::Style::default()
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(color::wash(scheme.on_surface, 0.07))),
            ..base
        },
        _ => base,
    }
}

/// A field caption over its control, grouped tight (`--space-1`).
pub fn labeled_field<'a, M: 'a>(
    label: impl Into<String>,
    control: Element<'a, M>,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    iced::widget::column![
        text(label.into())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
        control,
    ]
    .spacing(spacing::XS)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_builds_in_both_states() {
        let _: Element<'_, bool> = toggle(true, |v| v, &color::LIGHT);
        let _: Element<'_, bool> = toggle(false, |v| v, &color::LIGHT);
    }

    #[test]
    fn segment_active_uses_accent_fill() {
        let style = segment_style(&color::LIGHT, true, button::Status::Active);
        assert_eq!(
            style.background,
            Some(Background::Color(color::LIGHT.primary))
        );
        assert_eq!(style.text_color, color::LIGHT.on_primary);
    }

    #[test]
    fn segment_inactive_is_transparent() {
        let style = segment_style(&color::LIGHT, false, button::Status::Active);
        assert!(style.background.is_none());
        assert_eq!(style.text_color, color::LIGHT.on_surface);
    }
}
