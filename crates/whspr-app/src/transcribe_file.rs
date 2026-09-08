//! Transcribe a user-chosen audio file straight to on-screen text for the
//! Hub's "Transcribe a file..." button. Reuses the same ASR + refiner
//! backends as live dictation (`crate::worker::build_asr_backend` /
//! `build_refiner`), but runs the pipeline *without* a `TextSink` -- the
//! result is shown in the Hub rather than typed into another app, so this
//! path never touches the OS text-injection machinery.

use std::path::PathBuf;

use whspr_config::Config;
use whspr_core::{AudioBuffer, Pipeline, RefineContext};

use crate::worker::{build_asr_backend, build_refiner};

/// The recognized text, the recorded clip's duration in seconds, and -- when
/// speaker attribution is possible and succeeded -- a single speaker
/// embedding over the whole clip (see [`compute_embedding`]). The embedding
/// is `None` whenever speaker fingerprinting is disabled, no model is
/// installed, or embedding failed; it is fed to
/// `crate::speakers::attribute_speaker` to resolve a speaker UUID for history.
pub type TranscribeOutcome = (String, f32, Option<Vec<f32>>);

/// Decodes + resamples `file`, then transcribes it. See [`run_transcribe_audio`].
pub async fn run_transcribe(file: PathBuf, config: Config) -> Result<TranscribeOutcome, String> {
    let decoded = whspr_audio::decode_wav(&file).map_err(|e| e.to_string())?;
    let audio = whspr_audio::resample_to_16k_mono(&decoded).map_err(|e| e.to_string())?;
    run_transcribe_audio(audio, config).await
}

/// Transcribes and refines an already-decoded 16kHz-mono `audio` buffer using
/// `config`'s backends (shared by the "Transcribe a file" button and the
/// in-app record button). Runs off the UI thread via `Task::perform`; any
/// failure (no model configured, inference error) comes back as a string.
/// Returns the recognized text, the recorded audio's duration, and an
/// optional per-clip speaker embedding, so callers can save a complete
/// history entry *and* attribute a speaker (see `crate::history` /
/// `crate::speakers::attribute_speaker`) instead of just the text.
pub async fn run_transcribe_audio(
    audio: AudioBuffer,
    config: Config,
) -> Result<TranscribeOutcome, String> {
    let asr = build_asr_backend(&config)?;
    let refiner = build_refiner(&config)?;
    let duration_secs = audio.duration_secs();

    // Fingerprint the speaker over the whole clip *before* the pipeline
    // consumes `audio` (it takes it by value). Only actually runs a model
    // when attribution is possible; otherwise -- or on failure -- it's `None`
    // and transcription proceeds unaffected.
    let embedding = compute_embedding(&audio, &config).await;

    // Resolve the same way live dictation does (`crate::worker::run`) so the
    // record-button/file path and the hotkey path never disagree about
    // which language whisper was told to use.
    let language = whspr_config::effective_language(&config.language_settings, &config.language);
    let pipeline = Pipeline::new(asr, refiner)
        .with_language(language)
        .with_translate(config.capture.translate);
    let ctx = RefineContext {
        instructions: Some(whspr_refine::effective_instructions(
            config.refine_settings.instructions.as_deref(),
        )),
        ..Default::default()
    };
    let text = pipeline.run(audio, &ctx).await.map_err(|e| e.to_string())?;
    Ok((text, duration_secs, embedding))
}

/// Computes one speaker embedding over the whole `audio` clip, but *only*
/// when speaker attribution is actually possible: `config.speaker.enabled`
/// is set and a real diarization model directory resolves
/// (`SherpaDiarizer::resolve_model_dir` -- from `[speaker].model_dir` or the
/// `SPEAKER_MODEL_DIR` env var). The blocking sherpa call runs on a blocking
/// thread (`spawn_blocking`) so it never stalls iced's async executor.
///
/// Returns `None` for every "can't or shouldn't" case -- disabled, no model,
/// a model that fails to load, an embedding error, or a panicked task --
/// logging real failures. A speaker fingerprint is never worth failing an
/// otherwise-good transcription over, so this deliberately swallows errors
/// into `None` rather than propagating them.
async fn compute_embedding(audio: &AudioBuffer, config: &Config) -> Option<Vec<f32>> {
    if !config.speaker.enabled {
        return None;
    }
    let model_dir =
        whspr_diarize::SherpaDiarizer::resolve_model_dir(config.speaker.model_dir.clone())?;
    let embedding_choice = config.speaker.embedding_model;
    let audio = audio.clone();

    let result = tokio::task::spawn_blocking(move || {
        let diarizer = whspr_diarize::SherpaDiarizer::new(model_dir, embedding_choice)?;
        diarizer.embed_clip(&audio)
    })
    .await;

    match result {
        Ok(Ok(embedding)) => Some(embedding),
        Ok(Err(e)) => {
            tracing::warn!("whspr: speaker embedding failed, leaving dictation unattributed: {e}");
            None
        }
        Err(e) => {
            tracing::warn!("whspr: speaker embedding task panicked, leaving dictation unattributed: {e}");
            None
        }
    }
}
