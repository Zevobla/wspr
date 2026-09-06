//! Short start/stop audio cues for dictation (AG-05): a synthesized tone
//! (no bundled audio asset -- see `Cargo.toml`'s dependency notes) played
//! through rodio on a detached task, off the worker's own loop. Every
//! failure (no output device, stream error) is guarded: this is cosmetic
//! feedback, never something that should interrupt dictation.

use std::time::Duration;

use rodio::source::SineWave;
use rodio::{DeviceSinkBuilder, Source};

/// Which cue to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Recording started.
    Start,
    /// Recording stopped (about to transcribe).
    Stop,
}

/// How long each tone plays, fade in/out included.
const TONE_DURATION: Duration = Duration::from_millis(120);
/// How much of `TONE_DURATION` is spent fading in/out, so the tone
/// doesn't click at either end.
const FADE: Duration = Duration::from_millis(20);
/// Kept deliberately quiet -- this is a cue, not an alert.
const AMPLITUDE: f32 = 0.2;

impl Cue {
    /// A rising tone for Start, a falling one for Stop -- the same
    /// direction most recorders use for "began"/"ended" cues.
    fn frequency_hz(self) -> f32 {
        match self {
            Cue::Start => 880.0, // A5
            Cue::Stop => 440.0,  // A4
        }
    }
}

/// Plays `cue`'s tone on a detached task, so the caller (the worker loop)
/// never blocks on audio I/O. `enabled` is checked here, before spawning
/// anything, so callers (`crate::worker`) can call this unconditionally
/// and let the setting live in one place.
pub fn play(cue: Cue, enabled: bool) {
    if !enabled {
        return;
    }

    tokio::spawn(async move {
        let sink = match DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => sink,
            Err(error) => {
                tracing::warn!("sound feedback unavailable: {error}");
                return;
            }
        };

        let tone = SineWave::new(cue.frequency_hz())
            .take_duration(TONE_DURATION)
            .amplify(AMPLITUDE)
            .fade_in(FADE)
            .fade_out(FADE);

        sink.mixer().add(tone);
        tokio::time::sleep(TONE_DURATION).await;
        // `sink` (and the cpal stream it owns) drops here, ending
        // playback cleanly rather than being cut off mid-tone.
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_is_higher_pitched_than_stop() {
        assert!(Cue::Start.frequency_hz() > Cue::Stop.frequency_hz());
    }

    #[test]
    fn play_disabled_does_not_spawn_anything() {
        // No tokio runtime is running in this sync test -- if `play`
        // ignored `enabled` and called `tokio::spawn` anyway, this would
        // panic ("must be called from the context of a Tokio runtime"),
        // proving the disabled check happens before that call.
        play(Cue::Start, false);
    }
}
