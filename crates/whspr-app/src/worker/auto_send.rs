//! Hands-free endpointing for `[capture].auto_send`. whspr is hold-to-talk;
//! with auto-send on, a pause in speech during one long hold ends the chunk
//! recorded so far (it is transcribed and injected) while recording carries
//! straight on into the next chunk, until the hotkey is released.

use std::time::Duration;

/// How long, after speech, the input level must stay at or below the VAD
/// threshold before the chunk so far is sent.
pub(super) const AUTO_SEND_SILENCE_MS: u64 = 1500;

/// How often the worker samples the capture level while auto-send is on.
pub(super) const AUTO_SEND_TICK: Duration = Duration::from_millis(100);

/// A pure speech-then-pause detector, fed one `(level, elapsed)` sample per
/// tick. Speech is a level above the threshold; it fires once
/// [`AUTO_SEND_SILENCE_MS`] have passed since the last speech sample with no
/// speech in between, then resets and waits for new speech.
#[derive(Debug, Clone)]
pub(super) struct AutoSendDetector {
    /// `[capture].vad_threshold`: RMS levels above this count as speech.
    threshold: f32,
    /// When speech was last heard, relative to the start of the hold;
    /// `None` until speech is heard (again, after firing).
    last_speech: Option<Duration>,
}

impl AutoSendDetector {
    /// A detector that has heard no speech yet.
    pub(super) fn new(threshold: f32) -> Self {
        Self {
            threshold,
            last_speech: None,
        }
    }

    /// Feeds the level sampled `elapsed` into the hold. Returns `true`
    /// exactly when this sample completes a long-enough pause after speech,
    /// i.e. the current chunk should be sent now.
    pub(super) fn observe(&mut self, level: f32, elapsed: Duration) -> bool {
        if level > self.threshold {
            self.last_speech = Some(elapsed);
            return false;
        }
        let pause = Duration::from_millis(AUTO_SEND_SILENCE_MS);
        match self.last_speech {
            Some(spoke_at) if elapsed.saturating_sub(spoke_at) >= pause => {
                self.last_speech = None;
                true
            }
            _ => false,
        }
    }

    /// Whether speech has been heard since the detector was created or last
    /// fired.
    pub(super) fn speech_seen(&self) -> bool {
        self.last_speech.is_some()
    }
}
