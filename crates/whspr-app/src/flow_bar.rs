//! The Flow Bar: a small, borderless, always-on-top overlay showing the
//! live dictation pipeline state (Idle / Recording / Thinking / Done).
//!
//! ## Known limitation
//! iced 0.14's `window::Settings` can express `decorations: false` (no
//! title bar/border) and `level: Level::AlwaysOnTop`, which is the closest
//! honest approximation of a native overlay HUD it offers. It does *not*
//! expose click-through/mouse-passthrough at window-creation time (there is
//! a `window::enable_mouse_passthrough` task for an already-open window,
//! but no equivalent `Settings` field), and there's no window-shadow or
//! rounded-corner control. This is good enough to be visually
//! non-intrusive and always-visible, but it's still a regular (if
//! borderless) OS window, not a true compositor-level overlay.

use iced::widget::{container, text};
use iced::window::{Level, Position, Settings};
use iced::{Color, Element, Length, Point, Size};
use whspr_core::PipelineState;

use crate::state::{Message, State};

/// Renders the Flow Bar overlay's content: just the current pipeline state,
/// centered, colored, and large enough to read at a glance.
pub fn view(state: &State) -> Element<'_, Message> {
    let (label, color) = display_for(state.pipeline_state);

    container(text(label).size(24).color(color))
        .center(Length::Fill)
        .into()
}

/// Window settings for the Flow Bar overlay: borderless, always-on-top,
/// small, docked near the top-right corner of the primary monitor.
pub fn window_settings() -> Settings {
    let size = Size::new(280.0, 64.0);

    Settings {
        size,
        decorations: false,
        resizable: false,
        minimizable: false,
        level: Level::AlwaysOnTop,
        position: Position::SpecificWith(|window_size, screen_size| {
            Point::new(screen_size.width - window_size.width - 24.0, 24.0)
        }),
        ..Settings::default()
    }
}

/// Maps a pipeline state to the label and background color the Flow Bar
/// shows for it, collapsing whspr-core's five-state pipeline down to the
/// simpler Idle/Recording/Thinking/Done(+Error) the user actually needs to
/// glance at. Kept pure and separate from the view so the mapping is
/// unit-testable without an iced runtime.
pub fn display_for(state: PipelineState) -> (&'static str, Color) {
    match state {
        PipelineState::Idle => ("Idle", Color::from_rgb8(0x6b, 0x72, 0x80)),
        PipelineState::Recording => ("Recording", Color::from_rgb8(0xdc, 0x26, 0x26)),
        PipelineState::Transcribing | PipelineState::Refining => {
            ("Thinking", Color::from_rgb8(0x25, 0x63, 0xeb))
        }
        PipelineState::Injecting => ("Done", Color::from_rgb8(0x16, 0xa3, 0x4a)),
        PipelineState::Error => ("Error", Color::from_rgb8(0xea, 0x58, 0x0c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribing_and_refining_both_show_as_thinking() {
        assert_eq!(display_for(PipelineState::Transcribing).0, "Thinking");
        assert_eq!(display_for(PipelineState::Refining).0, "Thinking");
        assert_eq!(
            display_for(PipelineState::Transcribing).1,
            display_for(PipelineState::Refining).1
        );
    }

    #[test]
    fn every_state_has_a_distinct_label_or_color_from_idle() {
        let idle = display_for(PipelineState::Idle);
        for state in [
            PipelineState::Recording,
            PipelineState::Transcribing,
            PipelineState::Injecting,
            PipelineState::Error,
        ] {
            assert_ne!(display_for(state), idle);
        }
    }
}
