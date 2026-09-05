//! ASR backend implementations. Everything here implements
//! `whspr_core::AsrBackend`; the pipeline never knows or cares which one it
//! got. Bodies are `todo!()` until the ASR team wires up the real
//! dependencies (whisper-rs, HTTP clients, ...) — this crate must keep
//! compiling without those heavy deps in the meantime.

use async_trait::async_trait;
use std::path::PathBuf;

use whspr_core::{AsrBackend, AsrOptions, AudioBuffer, Result, Transcript};

#[cfg(feature = "testkit")]
pub use whspr_core::testkit::MockAsr;

/// Local transcription via whisper.cpp (whisper-rs). Opt in the `whisper-rs`
/// workspace dep from this crate's own Cargo.toml when implementing.
pub struct WhisperLocal {
    pub model_path: PathBuf,
}

impl WhisperLocal {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }
}

#[async_trait]
impl AsrBackend for WhisperLocal {
    async fn transcribe(&self, _audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        todo!("whspr-asr: wire up whisper-rs in WhisperLocal::transcribe")
    }

    fn id(&self) -> &'static str {
        "whisper-local"
    }
}

/// Cloud transcription via the OpenAI API (e.g. `whisper-1` / `gpt-4o-transcribe`).
pub struct OpenAiAsr {
    pub api_key: String,
}

impl OpenAiAsr {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl AsrBackend for OpenAiAsr {
    async fn transcribe(&self, _audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        todo!("whspr-asr: wire up OpenAI transcription HTTP call")
    }

    fn id(&self) -> &'static str {
        "openai"
    }
}

/// Cloud transcription via the Deepgram API.
pub struct DeepgramAsr {
    pub api_key: String,
}

impl DeepgramAsr {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl AsrBackend for DeepgramAsr {
    async fn transcribe(&self, _audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        todo!("whspr-asr: wire up Deepgram transcription HTTP call")
    }

    fn id(&self) -> &'static str {
        "deepgram"
    }
}
