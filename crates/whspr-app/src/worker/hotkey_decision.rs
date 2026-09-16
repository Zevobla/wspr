//! Maps a debounced hotkey action onto what the worker does with its
//! capture. Split out of `worker/mod.rs` so the pure decision and its
//! tests sit apart from the async capture loop.

use whspr_inject::DebounceAction;

/// What the worker should do with the currently active capture (if any) in
/// response to a debounced hotkey action. Pure, so the CancelRecording-vs-
/// StopRecording distinction -- the entire reason this debounce wiring
/// exists -- is unit-testable without a real hotkey listener, microphone,
/// or pipeline run. Mirrors `whspr_inject`'s own pattern of pulling a
/// decision out as a pure fn (`map_hotkey_state`, `HotkeyDebouncer`'s
/// `is_real_hold`/`is_double_press`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureDecision {
    /// Start a new capture.
    Start,
    /// Finalize the active capture: stop it and run the pipeline.
    Finalize,
    /// Discard the active capture without running the pipeline -- the D-10
    /// outcome for a hold too short to be a real recording.
    Discard,
    /// Nothing to do.
    Ignore,
}

pub(super) fn capture_decision(action: DebounceAction, capture_active: bool) -> CaptureDecision {
    match action {
        DebounceAction::StartRecording => CaptureDecision::Start,
        DebounceAction::StopRecording if capture_active => CaptureDecision::Finalize,
        DebounceAction::CancelRecording if capture_active => CaptureDecision::Discard,
        // A Stop/Cancel with no active capture (a stray/duplicate event we
        // never started one for) and an explicit Ignore both mean the same
        // thing here: nothing to do.
        DebounceAction::StopRecording
        | DebounceAction::CancelRecording
        | DebounceAction::Ignore => CaptureDecision::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_recording_with_no_active_capture_starts_one() {
        assert_eq!(
            capture_decision(DebounceAction::StartRecording, false),
            CaptureDecision::Start
        );
    }

    #[test]
    fn stop_recording_with_active_capture_finalizes() {
        assert_eq!(
            capture_decision(DebounceAction::StopRecording, true),
            CaptureDecision::Finalize
        );
    }

    /// The whole reason this wiring exists: a too-short hold must discard
    /// the capture, not finalize it -- no pipeline run, no empty transcript.
    #[test]
    fn cancel_recording_discards_without_finalizing() {
        let decision = capture_decision(DebounceAction::CancelRecording, true);
        assert_eq!(decision, CaptureDecision::Discard);
        assert_ne!(decision, CaptureDecision::Finalize);
    }

    /// A Stop/Cancel with nothing active (a stray/duplicate event) must
    /// never start a phantom pipeline run.
    #[test]
    fn stop_or_cancel_without_active_capture_is_ignored() {
        assert_eq!(
            capture_decision(DebounceAction::StopRecording, false),
            CaptureDecision::Ignore
        );
        assert_eq!(
            capture_decision(DebounceAction::CancelRecording, false),
            CaptureDecision::Ignore
        );
    }

    #[test]
    fn debounced_ignore_action_is_ignored() {
        assert_eq!(
            capture_decision(DebounceAction::Ignore, true),
            CaptureDecision::Ignore
        );
        assert_eq!(
            capture_decision(DebounceAction::Ignore, false),
            CaptureDecision::Ignore
        );
    }
}
