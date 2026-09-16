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

/// Whether the stretch still being recorded when the hotkey is released
/// should be transcribed. Always, except when auto-send already sent earlier
/// chunks from this hold and no speech has been heard since the last one --
/// that remainder is just the trailing pause.
pub(super) fn remainder_worth_sending(
    auto_send: bool,
    chunks_sent: usize,
    speech_seen: bool,
) -> bool {
    !(auto_send && chunks_sent > 0 && !speech_seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: f32 = 0.01;
    const SPEECH: f32 = 0.2;
    const QUIET: f32 = 0.0;

    /// Feeds `(level, at_ms)` samples and returns the times (ms) it fired.
    fn fire_times(samples: impl IntoIterator<Item = (f32, u64)>) -> Vec<u64> {
        let mut detector = AutoSendDetector::new(THRESHOLD);
        samples
            .into_iter()
            .filter(|&(level, at)| detector.observe(level, Duration::from_millis(at)))
            .map(|(_, at)| at)
            .collect()
    }

    /// One sample every 100 ms from `from` to `to` (inclusive) at `level`.
    fn span(level: f32, from: u64, to: u64) -> impl Iterator<Item = (f32, u64)> {
        (from..=to).step_by(100).map(move |at| (level, at))
    }

    #[test]
    fn the_detector_fires_only_after_a_long_enough_pause_following_speech() {
        struct Case {
            name: &'static str,
            samples: Vec<(f32, u64)>,
            fires_at: Vec<u64>,
        }
        let cases = [
            Case {
                name: "no speech never fires",
                samples: span(QUIET, 0, 6000).collect(),
                fires_at: vec![],
            },
            Case {
                name: "speech then 1.4 s of silence does not fire",
                samples: span(SPEECH, 0, 1000)
                    .chain(span(QUIET, 1100, 2400))
                    .collect(),
                fires_at: vec![],
            },
            Case {
                name: "speech then 1.5 s of silence fires once",
                samples: span(SPEECH, 0, 1000)
                    .chain(span(QUIET, 1100, 6000))
                    .collect(),
                fires_at: vec![2500],
            },
            Case {
                name: "a level exactly at the threshold is silence",
                samples: span(SPEECH, 0, 0)
                    .chain(span(THRESHOLD, 100, 1500))
                    .collect(),
                fires_at: vec![1500],
            },
            Case {
                name: "speech inside the pause restarts it",
                samples: span(SPEECH, 0, 0)
                    .chain(span(QUIET, 100, 1000))
                    .chain(span(SPEECH, 1100, 1100))
                    .chain(span(QUIET, 1200, 2600))
                    .collect(),
                fires_at: vec![2600],
            },
            Case {
                name: "it resets after firing and fires again after new speech",
                samples: span(SPEECH, 0, 500)
                    .chain(span(QUIET, 600, 3000))
                    .chain(span(SPEECH, 3100, 3500))
                    .chain(span(QUIET, 3600, 6000))
                    .collect(),
                fires_at: vec![2000, 5000],
            },
        ];
        for case in cases {
            assert_eq!(fire_times(case.samples), case.fires_at, "{}", case.name);
        }
    }

    #[test]
    fn speech_seen_tracks_speech_since_the_last_send() {
        let mut detector = AutoSendDetector::new(THRESHOLD);
        assert!(!detector.speech_seen());
        detector.observe(SPEECH, Duration::ZERO);
        assert!(detector.speech_seen());
        assert!(detector.observe(QUIET, Duration::from_millis(AUTO_SEND_SILENCE_MS)));
        assert!(!detector.speech_seen());
    }
}
