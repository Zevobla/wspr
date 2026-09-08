//! Tray-state helpers shared by `crate::app`: driving the pipeline state and
//! the tray icon together, and arming/measuring the lingering "Done" glance
//! after a completed dictation. Extracted from `crate::app` (with their unit
//! tests, unchanged) to keep that file under its 600-line cap (AA-06); see
//! `crate::tray` for the tray icon itself and its `TrayVisual` buckets.

use crate::state::State;
use crate::tray::TrayVisual;

/// How long the tray's "Done" icon lingers after a completed dictation
/// before reverting -- see `crate::app`'s `tray_done_subscription` /
/// `Message::TrayDoneTick` and `Message::Worker`'s `Completed` arm, which
/// starts the linger.
pub const TRAY_DONE_LINGER: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether the tray's lingering "Done" display is still within its window
/// at `now`. Pure so the Idle-suppression branch in `crate::app`'s
/// `StateChanged` arm, and the revert in `TrayDoneTick`, are unit-testable
/// without a real clock tick.
pub fn tray_done_active(
    tray_done_until: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    tray_done_until.is_some_and(|until| now < until)
}

/// Drives `state.pipeline_state` and the tray together to `new`, clearing
/// any pending "Done" linger so the transition isn't clobbered by the next
/// `TrayDoneTick` -- the Record button's echo of the hotkey `StateChanged` arm.
pub fn set_pipeline_state(state: &mut State, new: whspr_core::PipelineState) {
    state.pipeline_state = new;
    state.tray_done_until = None;
    if let Some(tray) = &state.tray {
        tray.set_state(new);
    }
}

/// Starts the tray's lingering "Done" glance (the Done visual plus the
/// `TRAY_DONE_LINGER` window `TrayDoneTick` reverts). Shared by the hotkey
/// `WorkerEvent::Completed` arm and the button's `FileTranscribed(Ok)` arm.
pub fn begin_tray_done_linger(state: &mut State) {
    if let Some(tray) = &state.tray {
        tray.set_visual(TrayVisual::Done);
    }
    state.tray_done_until = Some(std::time::Instant::now() + TRAY_DONE_LINGER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_done_active_true_before_the_deadline() {
        let now = std::time::Instant::now();
        let until = now + std::time::Duration::from_secs(2);
        assert!(tray_done_active(Some(until), now));
    }

    #[test]
    fn tray_done_active_false_after_the_deadline() {
        let now = std::time::Instant::now();
        let until = now - std::time::Duration::from_millis(1);
        assert!(!tray_done_active(Some(until), now));
    }

    #[test]
    fn tray_done_active_false_when_nothing_pending() {
        assert!(!tray_done_active(None, std::time::Instant::now()));
    }

    #[test]
    fn begin_tray_done_linger_arms_the_linger_window() {
        let mut state = State::new(whspr_config::Config::default());
        let before = std::time::Instant::now();
        begin_tray_done_linger(&mut state);
        assert!(state.tray_done_until.is_some_and(|until| until >= before));
    }

    #[test]
    fn set_pipeline_state_updates_state_and_clears_linger() {
        let mut state = State::new(whspr_config::Config::default());
        state.tray_done_until = Some(std::time::Instant::now());
        set_pipeline_state(&mut state, whspr_core::PipelineState::Recording);
        assert_eq!(state.pipeline_state, whspr_core::PipelineState::Recording);
        assert!(state.tray_done_until.is_none());
    }
}
