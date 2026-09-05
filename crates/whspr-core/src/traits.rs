use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AsrOptions, AudioBuffer, RefineContext, SpeakerTurn, Transcript};

/// Turns audio into text. Implemented by local (whisper.cpp) and cloud
/// (OpenAI, Deepgram, ...) backends in `whspr-asr`.
#[async_trait]
pub trait AsrBackend: Send + Sync {
    async fn transcribe(&self, audio: &AudioBuffer, opts: &AsrOptions) -> Result<Transcript>;

    /// Stable identifier used for config/CLI backend selection (e.g. "whisper-local").
    fn id(&self) -> &'static str;
}

/// Cleans up raw ASR output (punctuation, filler words, formatting) with
/// awareness of surrounding context. Implemented by local and cloud LLM
/// backends in `whspr-refine`.
#[async_trait]
pub trait TextRefiner: Send + Sync {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String>;

    fn id(&self) -> &'static str;
}

/// A push-to-talk / toggle key transition, as delivered by a `HotkeyListener`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

/// Watches for the configured global hotkey and streams press/release events.
/// Implemented in `whspr-inject` on top of the OS-level hotkey APIs.
pub trait HotkeyListener: Send + Sync {
    /// Subscribe to hotkey events for as long as the returned receiver (and
    /// the listener) stay alive. Multiple subscribers may be supported by
    /// implementations that broadcast internally.
    fn subscribe(&self) -> tokio::sync::mpsc::Receiver<HotkeyEvent>;
}

/// Delivers finished text to wherever the user is typing. Implemented in
/// `whspr-inject` via synthetic keystrokes / clipboard paste.
pub trait TextSink: Send + Sync {
    fn insert(&self, text: &str) -> Result<()>;
}

/// Detects and separates distinct speakers in an audio recording, returning
/// each speaker turn's timing and embedding vector for later matching
/// against an enrolled-speaker database. Implemented by a sherpa-onnx-backed
/// VAD/segmentation/embedding pipeline in `whspr-diarize`.
///
/// Unlike `AsrBackend`/`TextRefiner`, this is deliberately *not* `async`:
/// diarization backends are CPU-bound native inference calls (sherpa-onnx
/// FFI), not I/O-bound network calls — there's nothing to `.await` on.
/// Callers that need this off the async runtime's thread should wrap a call
/// in `tokio::task::spawn_blocking` themselves.
pub trait Diarizer: Send + Sync {
    fn diarize(&self, audio: &AudioBuffer) -> Result<Vec<SpeakerTurn>>;

    /// Stable identifier used for config/CLI backend selection.
    fn id(&self) -> &'static str;
}
