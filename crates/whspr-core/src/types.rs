use serde::{Deserialize, Serialize};

/// 16kHz mono f32 PCM audio. Capture/decode/resample all normalize down to
/// this shape before anything touches an ASR backend.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// One detected speaker turn from a `Diarizer` pass over an audio buffer:
/// its timing, an embedding vector for matching against an
/// enrolled-speaker database, an optional provisional speaker label (if
/// the backend does its own coarse clustering), and a confidence score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub start_secs: f32,
    pub end_secs: f32,
    pub embedding: Vec<f32>,
    pub speaker: Option<String>,
    pub score: f32,
}

/// One timed span of a transcript, as produced by ASR backends that support
/// word/segment-level timing. Backends that don't may leave this empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub start_secs: f32,
    pub end_secs: f32,
    /// Speaker label for this segment, if diarization has been run and
    /// matched against it. `None` for plain transcription — the live
    /// dictation pipeline never sets this; it's populated by the separate
    /// offline diarization analysis flow (see `whspr-diarize`).
    #[serde(default)]
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
}

/// Hints passed down to an `AsrBackend` for a single transcription call.
#[derive(Debug, Clone, Default)]
pub struct AsrOptions {
    pub language: Option<String>,
}

/// Context handed to a `TextRefiner` so it can clean up raw ASR output with
/// awareness of where the text is going.
#[derive(Debug, Clone, Default)]
pub struct RefineContext {
    pub app_name: Option<String>,
    pub prior_text: Option<String>,
    pub instructions: Option<String>,
}

/// Coarse state of the dictation pipeline, reported so UI/tray layers can
/// reflect what's happening without polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    Idle,
    Recording,
    Transcribing,
    Refining,
    Injecting,
    Error,
}
