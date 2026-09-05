//! Debounces raw hotkey press/release events before they drive a recording.
//!
//! Two things go wrong with unfiltered global-hotkey events:
//!
//! * a *very short* tap (press then release within a few dozen ms) would
//!   start and immediately stop a recording, producing an empty transcript;
//! * a *double-press* (two quick taps, or a second press while the first is
//!   still held) would start a second recording on top of the first.
//!
//! [`HotkeyDebouncer`] is a small state machine that turns timestamped
//! press/release events into [`DebounceAction`]s, dropping both of those
//! cases. Timestamps are passed in rather than read from the clock inside,
//! so the logic is deterministic and unit-testable.

use std::time::{Duration, Instant};

use whspr_core::HotkeyEvent;

/// Minimum press-to-release duration for a hold to count as a real
/// recording. Anything shorter is treated as an accidental tap. (D-10)
const MIN_HOLD: Duration = Duration::from_millis(200);

/// Two presses closer together than this count as a single double-press:
/// the second is ignored rather than starting a new recording. (D-09)
const DOUBLE_PRESS_WINDOW: Duration = Duration::from_millis(300);

/// What a debounced hotkey transition means for the recording pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceAction {
    /// Begin capturing audio: a fresh, accepted press.
    StartRecording,
    /// Finalize the current recording: a hold long enough to keep.
    StopRecording,
    /// Discard the current recording: the hold was too short to be real.
    CancelRecording,
    /// Nothing to do — a duplicate press, or a release with no active hold.
    Ignore,
}

/// Whether a hold of `duration` is long enough to keep as a recording.
fn is_real_hold(duration: Duration) -> bool {
    duration >= MIN_HOLD
}

/// Whether a press at `now` lands within the double-press window of the
/// previous press at `last_press` — i.e. it's the second tap of a
/// double-press and should be ignored.
fn is_double_press(last_press: Option<Instant>, now: Instant) -> bool {
    last_press.is_some_and(|previous| now.saturating_duration_since(previous) < DOUBLE_PRESS_WINDOW)
}

/// A small state machine that debounces raw hotkey events.
///
/// Feed it each [`HotkeyEvent`] together with the time it arrived; it
/// returns the [`DebounceAction`] to take. Construct one per hotkey stream
/// and drive it with events in the order they occur.
#[derive(Debug, Default)]
pub struct HotkeyDebouncer {
    /// When a hold is active, the time its press was accepted.
    active_since: Option<Instant>,
    /// The time of the most recent press event seen (accepted or not),
    /// used to detect double-presses.
    last_press: Option<Instant>,
}

impl HotkeyDebouncer {
    /// Creates a debouncer with no press in progress.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one timestamped event through the debouncer, returning the
    /// action the recording pipeline should take.
    pub fn on_event(&mut self, event: HotkeyEvent, now: Instant) -> DebounceAction {
        match event {
            HotkeyEvent::Pressed => self.on_press(now),
            HotkeyEvent::Released => self.on_release(now),
        }
    }

    fn on_press(&mut self, now: Instant) -> DebounceAction {
        let double = is_double_press(self.last_press, now);
        self.last_press = Some(now);

        if self.active_since.is_some() || double {
            // Already holding, or the second tap of a double-press — either
            // way, don't start a second recording. (D-09)
            DebounceAction::Ignore
        } else {
            self.active_since = Some(now);
            DebounceAction::StartRecording
        }
    }

    fn on_release(&mut self, now: Instant) -> DebounceAction {
        match self.active_since.take() {
            // A release with no active hold (e.g. the release of a press we
            // ignored) has nothing to finalize.
            None => DebounceAction::Ignore,
            Some(pressed_at) => {
                if is_real_hold(now.saturating_duration_since(pressed_at)) {
                    DebounceAction::StopRecording
                } else {
                    // Too short to be a real recording. (D-10)
                    DebounceAction::CancelRecording
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_real_hold_uses_the_min_hold_threshold() {
        assert!(!is_real_hold(Duration::ZERO));
        assert!(!is_real_hold(MIN_HOLD - Duration::from_millis(1)));
        assert!(is_real_hold(MIN_HOLD));
        assert!(is_real_hold(MIN_HOLD + Duration::from_millis(1)));
    }

    #[test]
    fn is_double_press_flags_only_presses_within_the_window() {
        let base = Instant::now();

        // No previous press can't be a double-press.
        assert!(!is_double_press(None, base));
        // A press just after the previous one is a double-press.
        assert!(is_double_press(Some(base), base + Duration::from_millis(10)));
        assert!(is_double_press(
            Some(base),
            base + DOUBLE_PRESS_WINDOW - Duration::from_millis(1)
        ));
        // At or beyond the window it's a separate, intentional press.
        assert!(!is_double_press(Some(base), base + DOUBLE_PRESS_WINDOW));
        assert!(!is_double_press(
            Some(base),
            base + DOUBLE_PRESS_WINDOW + Duration::from_millis(1)
        ));
    }
}
