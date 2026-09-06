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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_exported() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&v1, &v2);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn audio_buffer_exported_from_types() {
        let buf = AudioBuffer::new(vec![0.5], 16000);
        assert_eq!(buf.sample_rate, 16000);
    }

    #[test]
    fn hotkey_event_exported_from_traits() {
        let event = HotkeyEvent::Pressed;
        assert_eq!(event, HotkeyEvent::Pressed);
    }

    #[test]
    fn whspr_error_exported_from_error() {
        let err = WhsprError::Other("test".to_string());
        assert_eq!(err.to_string(), "test");
    }
}
