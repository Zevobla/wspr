//! Lightweight test doubles shared across the workspace so backend crates
//! (and the CLI) don't each reinvent a fake ASR/refiner for their own tests.
//! Gated behind the `testkit` feature; always compiled for this crate's own
//! `cfg(test)` builds regardless of feature selection.

use async_trait::async_trait;

use crate::error::Result;
use crate::traits::{AsrBackend, Diarizer, TextRefiner};
use crate::types::{AsrOptions, AudioBuffer, RefineContext, SpeakerTurn, Transcript};

/// Returns a canned `Transcript` for every call, ignoring the audio content.
pub struct MockAsr {
    pub canned: Transcript,
}

impl MockAsr {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            canned: Transcript {
                text: text.into(),
                ..Default::default()
            },
        }
    }
}

impl Default for MockAsr {
    fn default() -> Self {
        Self::new("the quick brown fox jumps over the lazy dog")
    }
}

#[async_trait]
impl AsrBackend for MockAsr {
    async fn transcribe(&self, _audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        Ok(self.canned.clone())
    }

    fn id(&self) -> &'static str {
        "mock"
    }
}

/// Passes text through unchanged.
pub struct NoopRefiner;

#[async_trait]
impl TextRefiner for NoopRefiner {
    async fn refine(&self, raw: &str, _ctx: &RefineContext) -> Result<String> {
        Ok(raw.to_string())
    }

    fn id(&self) -> &'static str {
        "noop"
    }
}

/// Returns canned `SpeakerTurn`s for every call, ignoring the audio content.
pub struct MockDiarizer {
    pub canned: Vec<SpeakerTurn>,
}

impl MockDiarizer {
    pub fn new(turns: Vec<SpeakerTurn>) -> Self {
        Self { canned: turns }
    }
}

impl Default for MockDiarizer {
    fn default() -> Self {
        Self::new(vec![
            SpeakerTurn {
                start_secs: 0.0,
                end_secs: 2.5,
                embedding: vec![0.1, 0.2, 0.3],
                speaker: None,
                score: 0.95,
            },
            SpeakerTurn {
                start_secs: 2.5,
                end_secs: 5.0,
                embedding: vec![0.4, 0.5, 0.6],
                speaker: None,
                score: 0.92,
            },
        ])
    }
}

impl Diarizer for MockDiarizer {
    fn diarize(&self, _audio: &AudioBuffer) -> Result<Vec<SpeakerTurn>> {
        Ok(self.canned.clone())
    }

    fn id(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_diarizer_returns_its_canned_turns() {
        let mock = MockDiarizer::default();
        let audio = AudioBuffer::new(vec![0.0; 100], 16_000);
        let turns = mock.diarize(&audio).unwrap();
        assert_eq!(turns, mock.canned);
    }
}
