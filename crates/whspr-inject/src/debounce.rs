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

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use whspr_core::{HotkeyEvent, HotkeyListener};

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

/// A source of "now", injectable so the wrapper's timing is deterministic
/// under test.
type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Wraps a [`HotkeyListener`] and runs its events through a
/// [`HotkeyDebouncer`], exposing debounced [`DebounceAction`]s instead of
/// raw press/release events.
///
/// This is the drop-in seam for the recording loop: subscribe to
/// [`subscribe_actions`](Self::subscribe_actions) and act on
/// `StartRecording` / `StopRecording` / `CancelRecording`; short taps and
/// double-presses never reach you.
pub struct DebouncedHotkeyListener<L: HotkeyListener> {
    inner: L,
    clock: Clock,
}

impl<L: HotkeyListener> DebouncedHotkeyListener<L> {
    /// Wraps `inner`, timestamping each event with the system clock as it
    /// arrives.
    pub fn new(inner: L) -> Self {
        Self {
            inner,
            clock: Arc::new(Instant::now),
        }
    }

    /// Subscribes to the inner listener and returns a stream of debounced
    /// actions. `Ignore` outcomes (duplicate presses, stray releases) are
    /// filtered out and never forwarded.
    pub fn subscribe_actions(&self) -> mpsc::Receiver<DebounceAction> {
        let mut raw = self.inner.subscribe();
        let clock = Arc::clone(&self.clock);
        let (tx, rx) = mpsc::channel(16);

        // The inner listener delivers events on its own thread over a tokio
        // channel; bridge them synchronously through the debouncer.
        // `blocking_recv`/`blocking_send` need no ambient runtime here.
        thread::spawn(move || {
            let mut debouncer = HotkeyDebouncer::new();
            while let Some(event) = raw.blocking_recv() {
                match debouncer.on_event(event, (*clock)()) {
                    DebounceAction::Ignore => continue,
                    action => {
                        if tx.blocking_send(action).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        rx
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
        assert!(is_double_press(
            Some(base),
            base + Duration::from_millis(10)
        ));
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

    /// A normal hold — press, hold past the threshold, release — records
    /// once: start on press, stop on release.
    #[test]
    fn normal_hold_starts_then_stops_recording() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base),
            DebounceAction::StartRecording
        );
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Released, base + Duration::from_millis(500)),
            DebounceAction::StopRecording
        );
    }

    /// D-10: a press released before the min-hold threshold is cancelled,
    /// not committed, so no (empty) recording results.
    #[test]
    fn short_hold_is_cancelled_not_recorded() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base),
            DebounceAction::StartRecording
        );
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Released, base + Duration::from_millis(50)),
            DebounceAction::CancelRecording
        );
    }

    /// D-09: a second press while the first is still held is ignored, so the
    /// single hold still finalizes exactly once.
    #[test]
    fn second_press_while_holding_is_ignored() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base),
            DebounceAction::StartRecording
        );
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base + Duration::from_millis(20)),
            DebounceAction::Ignore
        );
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Released, base + Duration::from_millis(400)),
            DebounceAction::StopRecording
        );
    }

    /// D-09: a rapid double-tap (two quick press/release pairs) starts at
    /// most one recording — the second press falls inside the double-press
    /// window and is ignored.
    #[test]
    fn rapid_double_tap_does_not_start_two_recordings() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        let sequence = [
            (HotkeyEvent::Pressed, 0, DebounceAction::StartRecording),
            (HotkeyEvent::Released, 40, DebounceAction::CancelRecording),
            // Second tap lands within DOUBLE_PRESS_WINDOW of the first press.
            (HotkeyEvent::Pressed, 80, DebounceAction::Ignore),
            (HotkeyEvent::Released, 120, DebounceAction::Ignore),
        ];

        for (event, offset_ms, expected) in sequence {
            assert_eq!(
                debouncer.on_event(event, base + Duration::from_millis(offset_ms)),
                expected,
                "event {event:?} at +{offset_ms}ms",
            );
        }
    }

    /// Two deliberate presses spaced beyond the double-press window each
    /// start their own recording — debouncing doesn't swallow real repeats.
    #[test]
    fn separate_presses_after_the_window_each_record() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base),
            DebounceAction::StartRecording
        );
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Released, base + Duration::from_millis(400)),
            DebounceAction::StopRecording
        );

        // Well past the window since the previous press: a fresh recording.
        let second = Duration::from_millis(400) + DOUBLE_PRESS_WINDOW + Duration::from_millis(10);
        assert_eq!(
            debouncer.on_event(HotkeyEvent::Pressed, base + second),
            DebounceAction::StartRecording
        );
        assert_eq!(
            debouncer.on_event(
                HotkeyEvent::Released,
                base + second + Duration::from_millis(400)
            ),
            DebounceAction::StopRecording
        );
    }

    /// A release with no press in progress (a stray event) is ignored.
    #[test]
    fn release_without_active_press_is_ignored() {
        let base = Instant::now();
        let mut debouncer = HotkeyDebouncer::new();

        assert_eq!(
            debouncer.on_event(HotkeyEvent::Released, base),
            DebounceAction::Ignore
        );
    }

    /// A [`HotkeyListener`] that replays a fixed script of raw events on its
    /// own thread, then closes the channel — enough to drive the wrapper
    /// without a real display or OS hotkey.
    struct ScriptedListener {
        events: Vec<HotkeyEvent>,
    }

    impl HotkeyListener for ScriptedListener {
        fn subscribe(&self) -> mpsc::Receiver<HotkeyEvent> {
            let (tx, rx) = mpsc::channel(16);
            let events = self.events.clone();
            thread::spawn(move || {
                for event in events {
                    if tx.blocking_send(event).is_err() {
                        return;
                    }
                }
            });
            rx
        }
    }

    /// A clock that hands out `times` in call order, deterministically
    /// pairing each raw event with the timestamp the wrapper should see.
    fn scripted_clock(times: Vec<Instant>) -> Clock {
        let next = std::sync::Mutex::new(0usize);
        Arc::new(move || {
            let mut i = next.lock().unwrap();
            let time = times[(*i).min(times.len() - 1)];
            *i += 1;
            time
        })
    }

    /// End-to-end through the wrapper: a rapid double-tap yields a single
    /// (cancelled) recording — the second tap is filtered out entirely, and
    /// `Ignore` outcomes never reach the action stream.
    #[test]
    fn debounced_listener_filters_a_rapid_double_tap() {
        let base = Instant::now();
        let listener = DebouncedHotkeyListener {
            inner: ScriptedListener {
                events: vec![
                    HotkeyEvent::Pressed,
                    HotkeyEvent::Released,
                    HotkeyEvent::Pressed,
                    HotkeyEvent::Released,
                ],
            },
            clock: scripted_clock(vec![
                base,
                base + Duration::from_millis(40),
                base + Duration::from_millis(80),
                base + Duration::from_millis(120),
            ]),
        };

        let mut actions = listener.subscribe_actions();
        let mut collected = Vec::new();
        while let Some(action) = actions.blocking_recv() {
            collected.push(action);
        }

        assert_eq!(
            collected,
            vec![
                DebounceAction::StartRecording,
                DebounceAction::CancelRecording
            ]
        );
    }
}
