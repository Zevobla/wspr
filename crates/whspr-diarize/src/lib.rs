//! Speaker fingerprinting (diarization) backend for whspr, wrapping
//! `sherpa-rs` (Rust bindings to k2-fsa/sherpa-onnx) to find speaker turns
//! and extract one embedding vector per turn.
//!
//! # What this crate does *not* do
//!
//! Speaker *identity* resolution — matching a turn's embedding against a
//! persisted database of previously-enrolled speakers, deciding "is this
//! speaker #3 again or someone new" — happens downstream, in a separate
//! `SpeakerDb` module. [`SherpaDiarizer::diarize`] always returns
//! `SpeakerTurn { speaker: None, .. }`; identity is resolved later by
//! whoever consumes the turns. This crate's only job is turn-finding +
//! embedding extraction (phased v1: no word-level who-said-what alignment,
//! no live speaker-ID — those are v2).
//!
//! # Approach: bundled segmentation pipeline + a separate embedding re-extract
//!
//! `sherpa-rs` exposes a bundled "offline speaker diarization" pipeline
//! (`sherpa_rs::diarize::Diarize`) that internally runs a pyannote
//! speaker-segmentation model, a speaker-embedding model, and its own
//! threshold-based clustering, all in one `compute()` call. We use it for
//! turn *timing* (real speaker-change segmentation, not just raw VAD — a
//! strictly better v1 starting point than treating every VAD speech span as
//! a turn). We deliberately discard its clustering output (an opaque
//! integer cluster id per segment, not stable across calls and not what
//! `SpeakerTurn::speaker` means downstream) rather than trying to force it
//! into `Option<String>`.
//!
//! However, `Diarize::compute` only returns `{ start, end, speaker: i32 }`
//! per segment — the public Rust API does **not** expose the internal
//! embedding vector sherpa computes for each segment while clustering. To
//! populate `SpeakerTurn::embedding` (which downstream `SpeakerDb` matching
//! needs), we load a *second*, independent `sherpa_rs::speaker_id::
//! EmbeddingExtractor` from the same embedding model file and re-run it over
//! each segment's audio slice to get a real per-turn embedding. This means
//! the embedding model is loaded twice (once inside `Diarize`, once as our
//! own extractor) and each segment's audio passes through the embedding
//! network twice — an acceptable v1 cost for CPU-bound offline analysis
//! (this is not on the live-dictation hot path), traded for correctness (an
//! actual, comparable embedding per turn) over trying to reach through the
//! FFI boundary for sherpa's internal per-segment vectors.
//!
//! `SpeakerTurn::score` is set to `1.0` for every turn: neither the
//! segmentation model nor the clustering step exposes a real per-segment
//! confidence through this API surface.
//!
//! # Required model files
//!
//! `SherpaDiarizer::new(model_dir)` expects `model_dir` to contain exactly
//! two files (literal names, chosen by this crate — rename the upstream
//! release assets to match):
//!
//! - `segmentation.onnx` — the pyannote speaker-segmentation-3.0 model.
//!   Source: rename `model.onnx` extracted from
//!   <https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2>
//!   (sherpa-onnx model zoo, `speaker-segmentation-models` release).
//! - `embedding.onnx` — a WeSpeaker CAM++ speaker-embedding model trained on
//!   VoxCeleb (English). Source: rename
//!   `wespeaker_en_voxceleb_CAM++.onnx` downloaded from
//!   <https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_CAM%2B%2B.onnx>
//!   (sherpa-onnx model zoo, `speaker-recongition-models` release — note the
//!   upstream release tag's spelling).
//!
//! See `examples/verify.rs` in this crate for a manual, real-model
//! verification harness (deliberately not part of `cargo test --workspace`,
//! which must stay offline and model-free).

use std::path::Path;
use std::sync::Mutex;

use sherpa_rs::diarize::{Diarize, DiarizeConfig};
use sherpa_rs::speaker_id::{EmbeddingExtractor, ExtractorConfig};

use whspr_core::{AudioBuffer, Diarizer, Result, SpeakerTurn, WhsprError};

/// Filename `model_dir` must contain for the pyannote segmentation model.
pub const SEGMENTATION_MODEL_FILENAME: &str = "segmentation.onnx";
/// Filename `model_dir` must contain for the speaker embedding model.
pub const EMBEDDING_MODEL_FILENAME: &str = "embedding.onnx";

/// `whspr_core::Diarizer` backed by sherpa-onnx (via `sherpa-rs`). See the
/// module docs for exactly which model files it needs and how it derives
/// per-turn embeddings.
pub struct SherpaDiarizer {
    diarize: Mutex<Diarize>,
    embedder: Mutex<EmbeddingExtractor>,
}

impl SherpaDiarizer {
    /// Loads the segmentation + embedding models from `model_dir`. See the
    /// module doc comment for the exact filenames expected.
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        let model_dir = model_dir.as_ref();
        if !model_dir.is_dir() {
            return Err(WhsprError::Diarize(format!(
                "diarization model_dir {} does not exist or is not a directory",
                model_dir.display()
            )));
        }

        let segmentation_model = model_dir.join(SEGMENTATION_MODEL_FILENAME);
        let embedding_model = model_dir.join(EMBEDDING_MODEL_FILENAME);
        for (label, path) in [
            ("segmentation model", &segmentation_model),
            ("embedding model", &embedding_model),
        ] {
            if !path.is_file() {
                return Err(WhsprError::Diarize(format!(
                    "{label} not found at {} (see whspr-diarize crate docs for required model filenames)",
                    path.display()
                )));
            }
        }

        let diarize_config = DiarizeConfig {
            // `Some(-1)` (not `None`, which the sherpa-rs wrapper defaults to
            // 4 — see `DiarizeConfig::default`) enables sherpa's
            // threshold-based clustering so the number of speakers is
            // discovered rather than assumed fixed. We only use `Diarize`
            // for turn *timing*; the cluster ids it produces are discarded
            // (see module docs).
            num_clusters: Some(-1),
            ..Default::default()
        };
        let diarize =
            Diarize::new(&segmentation_model, &embedding_model, diarize_config).map_err(|e| {
                WhsprError::Diarize(format!("failed to load sherpa diarization pipeline: {e}"))
            })?;

        let embedder = EmbeddingExtractor::new(ExtractorConfig {
            model: embedding_model.to_string_lossy().into_owned(),
            ..Default::default()
        })
        .map_err(|e| {
            WhsprError::Diarize(format!("failed to load speaker embedding extractor: {e}"))
        })?;

        Ok(Self {
            diarize: Mutex::new(diarize),
            embedder: Mutex::new(embedder),
        })
    }
}

/// Clamps a turn's `[start_secs, end_secs)` span to a valid sample range
/// within `total_samples` at `sample_rate`. Pure/deterministic so it can be
/// unit-tested without touching the sherpa FFI boundary.
fn segment_sample_range(
    start_secs: f32,
    end_secs: f32,
    sample_rate: u32,
    total_samples: usize,
) -> (usize, usize) {
    let to_sample = |secs: f32| -> usize {
        if secs <= 0.0 {
            0
        } else {
            (secs * sample_rate as f32).round() as usize
        }
    };
    let start = to_sample(start_secs).min(total_samples);
    let end = to_sample(end_secs).min(total_samples).max(start);
    (start, end)
}

impl Diarizer for SherpaDiarizer {
    fn diarize(&self, audio: &AudioBuffer) -> Result<Vec<SpeakerTurn>> {
        let sample_rate = audio.sample_rate;

        // `Diarize::compute` takes ownership of the sample buffer, so clone
        // it up front; we still need the original samples afterwards to
        // slice out each segment's audio for the embedding re-extract.
        let segments = {
            let mut diarize = self
                .diarize
                .lock()
                .map_err(|_| WhsprError::Diarize("diarization pipeline lock poisoned".into()))?;
            diarize
                .compute(audio.samples.clone(), None)
                .map_err(|e| WhsprError::Diarize(format!("diarization failed: {e}")))?
        };

        let mut embedder = self
            .embedder
            .lock()
            .map_err(|_| WhsprError::Diarize("embedding extractor lock poisoned".into()))?;

        let mut turns = Vec::with_capacity(segments.len());
        for segment in segments {
            let (start_sample, end_sample) =
                segment_sample_range(segment.start, segment.end, sample_rate, audio.samples.len());
            if start_sample >= end_sample {
                // Degenerate segment (e.g. clamped entirely outside the
                // audio, or a zero-duration segment) — nothing to embed, so
                // drop it rather than emit a `SpeakerTurn` with an empty
                // (dimension-0) embedding that would look inconsistent to
                // downstream consumers.
                continue;
            }
            let slice = audio.samples[start_sample..end_sample].to_vec();

            let embedding = embedder
                .compute_speaker_embedding(slice, sample_rate)
                .map_err(|e| {
                    WhsprError::Diarize(format!(
                        "embedding extraction failed for segment [{:.2}, {:.2}]: {e}",
                        segment.start, segment.end
                    ))
                })?;

            turns.push(SpeakerTurn {
                start_secs: segment.start,
                end_secs: segment.end,
                embedding,
                speaker: None,
                score: 1.0,
            });
        }

        Ok(turns)
    }

    fn id(&self) -> &'static str {
        "sherpa"
    }
}
