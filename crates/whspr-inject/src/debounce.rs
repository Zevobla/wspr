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
