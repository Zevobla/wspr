//! Lightweight test doubles shared across the workspace so backend crates
//! (and the CLI) don't each reinvent a fake ASR/refiner for their own tests.
//! Gated behind the `testkit` feature; always compiled for this crate's own
//! `cfg(test)` builds regardless of feature selection.

use async_trait::async_trait;

use crate::error::Result;
use crate::traits::{AsrBackend, TextRefiner};
use crate::types::{AsrOptions, AudioBuffer, RefineContext, Transcript};

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
