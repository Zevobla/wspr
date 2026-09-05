//! The spine of whspr. Every other crate depends on this one and only this
//! one for the shared domain types and traits — it must never depend back on
//! a leaf crate (asr/refine/audio/inject/config).

mod error;
mod pipeline;
mod similarity;
mod traits;
mod types;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use error::{Result, WhsprError};
pub use pipeline::{Pipeline, StateCallback};
pub use similarity::cosine_similarity;
pub use traits::{AsrBackend, Diarizer, HotkeyEvent, HotkeyListener, TextRefiner, TextSink};
pub use types::{
    AsrOptions, AudioBuffer, PipelineState, RefineContext, SpeakerTurn, Transcript,
    TranscriptSegment,
};
