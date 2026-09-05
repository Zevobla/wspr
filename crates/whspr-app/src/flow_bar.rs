//! The Flow Bar: a small, borderless, always-on-top overlay showing the
//! live dictation pipeline state (Idle / Recording / Thinking / Done),
//! restyled on the MD3 tokens in `crate::theme` with a per-state
//! animation: Recording breathes, Thinking sweeps a progress indicator,
//! and Done fades in -- see `animate` for how each is driven.
//!
//! ## Known limitation
//! iced 0.14's `window::Settings` can express `decorations: false` (no
//! title bar/border) and `level: Level::AlwaysOnTop`, which is the closest
//! honest approximation of a native overlay HUD it offers. It does *not*
//! expose click-through/mouse-passthrough at window-creation time (there is
//! a `window::enable_mouse_passthrough` task for an already-open window,
//! but no equivalent `Settings` field), and there's no window-shadow or
//! rounded-corner control at the OS level -- `styles::container::flow_bar`
//! draws its own rounded corners, border, and shadow on the content
//! instead. This is good enough to be visually non-intrusive and
//! always-visible, but it's still a regular (if borderless) OS window, not
//! a true compositor-level overlay.

use iced::widget::{column, container, progress_bar, text};
use iced::window::{Level, Position, Settings};
use iced::{Alignment, Color, Element, Length, Point, Size};
use std::time::Duration;
use whspr_core::PipelineState;

use crate::state::{Message, State};
use crate::theme::{self, color, motion, spacing, styles, type_scale, Scheme};

/// Renders the Flow Bar overlay's content: the current pipeline state's
/// label, centered, colored, and large enough to read at a glance, plus a
/// sweeping progress indicator while transcribing/refining.
pub fn view(state: &State) -> Element<'_, Message> {
    let scheme = theme::scheme(&state.theme);
    let elapsed = state.pipeline_state_since.elapsed();
    let (base_fill, text_color) = base_colors_for(state.pipeline_state, scheme);
    let fill = animate(state.pipeline_state, base_fill, scheme, elapsed);

    let label = text(label_for(state.pipeline_state))
        .size(type_scale::TITLE_LARGE.emphasized().size)
        .font(type_scale::TITLE_LARGE.emphasized().font())
        .color(text_color);

    let content: Element<'_, Message> = if matches!(
        state.pipeline_state,
        PipelineState::Transcribing | PipelineState::Refining
    ) {
        column![
            label,
            progress_bar(0.0..=1.0, sweep_phase(elapsed))
                .length(Length::Fixed(160.0))
                .girth(Length::Fixed(4.0))
                .style(move |_theme| styles::progress_bar::thinking(scheme)),
        ]
        .spacing(spacing::XS)
        .align_x(Alignment::Center)
        .into()
    } else {
        label.into()
    };

    container(content)
        .padding(spacing::MD)
        .center(Length::Fill)
        .style(move |_theme| styles::container::flow_bar(fill, scheme))
        .into()
}

/// Window settings for the Flow Bar overlay: borderless, always-on-top,
/// small, docked near the top-right corner of the primary monitor.
pub fn window_settings() -> Settings {
    let size = Size::new(280.0, 72.0);

    Settings {
        size,
        decorations: false,
        resizable: false,
        minimizable: false,
        level: Level::AlwaysOnTop,
        position: Position::SpecificWith(|window_size, screen_size| {
            Point::new(
                screen_size.width - window_size.width - spacing::XL,
                spacing::XL,
            )
        }),
        ..Settings::default()
    }
}

/// The Flow Bar's label for a pipeline state, collapsing whspr-core's
/// five-state pipeline down to the simpler Idle/Recording/Thinking/
/// Done(+Error) the user actually needs to glance at.
fn label_for(state: PipelineState) -> &'static str {
    match state {
        PipelineState::Idle => "Idle",
        PipelineState::Recording => "Recording",
        PipelineState::Transcribing | PipelineState::Refining => "Thinking",
        PipelineState::Injecting => "Done",
        PipelineState::Error => "Error",
    }
}

/// The Flow Bar's resting (non-animated) container fill and text color for
/// a pipeline state, from the given MD3 color scheme. `view` layers a
/// per-state animation (see `animate`) on top of the fill before
/// rendering; the text color stays fixed since every animation here only
/// moves the container's fill, not its text.
fn base_colors_for(state: PipelineState, scheme: &Scheme) -> (Color, Color) {
    match state {
        PipelineState::Idle => (scheme.surface_container, scheme.on_surface_variant),
        PipelineState::Recording => (scheme.error_container, scheme.on_error_container),
        PipelineState::Transcribing | PipelineState::Refining => {
            (scheme.tertiary_container, scheme.on_tertiary_container)
        }
        PipelineState::Injecting => (scheme.success_container, scheme.on_success_container),
        PipelineState::Error => (scheme.error, scheme.on_error),
    }
}

/// Applies the state's animation on top of its resting fill: Recording
/// breathes toward `error`, Injecting fades in from Idle's resting fill.
/// Every other state stays static -- Thinking's motion lives entirely in
/// its progress indicator (see `sweep_phase`), and Error is a rare,
/// important alert that doesn't need motion to earn attention.
fn animate(state: PipelineState, base: Color, scheme: &Scheme, elapsed: Duration) -> Color {
    match state {
        PipelineState::Recording => pulse(base, scheme.error, elapsed),
        PipelineState::Injecting => {
            let (from, _) = base_colors_for(PipelineState::Idle, scheme);
            fade_in(from, base, elapsed)
        }
        _ => base,
    }
}

/// A smooth breathing pulse: mixes up to 35% of `accent` into `base` on a
/// sine wave, capped well short of 100% so the fixed text color on top
/// stays legible at every point in the cycle. Not part of MD3's
/// easing/duration token system (that's for transitions, not loops -- see
/// `crate::theme::motion`'s module docs) -- only the cycle's period comes
/// from that scale.
fn pulse(base: Color, accent: Color, elapsed: Duration) -> Color {
    let period = motion::LONG_2.as_secs_f32() * 2.0;
    let phase = (elapsed.as_secs_f32() % period) / period;
    let wave = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;

    color::state_layer(base, accent, wave * 0.35)
}

/// A one-shot cross-fade from `from` to `to`, eased with MD3's "emphasized
/// decelerate" curve over `motion::MEDIUM_4` -- the spec's own pairing for
/// "element enters screen".
fn fade_in(from: Color, to: Color, elapsed: Duration) -> Color {
    let t = elapsed.as_secs_f32() / motion::MEDIUM_4.as_secs_f32();
    let eased = motion::CubicBezier::EMPHASIZED_DECELERATE.ease(t);

    color::lerp(from, to, eased)
}

/// The Thinking progress indicator's fill phase: a repeating sweep, eased
/// with MD3's "emphasized" curve so each pass accelerates then decelerates
/// instead of moving at a mechanical constant rate.
fn sweep_phase(elapsed: Duration) -> f32 {
    let period = motion::EXTRA_LONG_2.as_secs_f32();
    let t = (elapsed.as_secs_f32() % period) / period;

    motion::CubicBezier::EMPHASIZED.ease(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribing_and_refining_both_show_as_thinking() {
        assert_eq!(label_for(PipelineState::Transcribing), "Thinking");
        assert_eq!(label_for(PipelineState::Refining), "Thinking");
    }

    #[test]
    fn every_state_has_a_distinct_label_from_idle() {
        let idle = label_for(PipelineState::Idle);
        for state in [
            PipelineState::Recording,
            PipelineState::Transcribing,
            PipelineState::Injecting,
            PipelineState::Error,
        ] {
            assert_ne!(label_for(state), idle);
        }
    }

    #[test]
    fn every_state_has_distinct_base_colors_from_idle() {
        let scheme = &color::LIGHT;
        let idle = base_colors_for(PipelineState::Idle, scheme);
        for state in [
            PipelineState::Recording,
            PipelineState::Transcribing,
            PipelineState::Injecting,
            PipelineState::Error,
        ] {
            assert_ne!(base_colors_for(state, scheme), idle);
        }
    }

    #[test]
    fn fade_in_starts_at_from_and_reaches_to() {
        let from = Color::from_rgb8(0, 0, 0);
        let to = Color::from_rgb8(255, 255, 255);

        assert_eq!(fade_in(from, to, Duration::ZERO), from);
        let end = fade_in(from, to, motion::MEDIUM_4);
        assert!((end.r - to.r).abs() < 0.01);
    }

    #[test]
    fn pulse_never_exceeds_the_35_percent_cap() {
        let base = color::LIGHT.error_container;
        let accent = color::LIGHT.error;

        // Sample across a full cycle; every point should stay strictly
        // between `base` and a 35%-mixed color, never reaching `accent`.
        for step in 0..20 {
            let elapsed = Duration::from_millis(step * 50);
            let pulsed = pulse(base, accent, elapsed);
            let capped = color::state_layer(base, accent, 0.35);

            let distance = (pulsed.r - base.r).abs();
            let max_distance = (capped.r - base.r).abs();
            assert!(distance <= max_distance + 1e-4);
        }
    }

    #[test]
    fn sweep_phase_is_periodic() {
        let period_millis = motion::EXTRA_LONG_2.as_millis() as u64;
        let a = sweep_phase(Duration::from_millis(100));
        let b = sweep_phase(Duration::from_millis(100 + period_millis));
        assert!((a - b).abs() < 1e-4);
    }
}
