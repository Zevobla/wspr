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

/// Decodes + resamples `file`, then transcribes it. See [`run_transcribe_audio`].
pub async fn run_transcribe(file: PathBuf, config: Config) -> Result<String, String> {
    let decoded = whspr_audio::decode_wav(&file).map_err(|e| e.to_string())?;
    let audio = whspr_audio::resample_to_16k_mono(&decoded).map_err(|e| e.to_string())?;
    run_transcribe_audio(audio, config).await
}

/// Transcribes and refines an already-decoded 16kHz-mono `audio` buffer using
/// `config`'s backends (shared by the "Transcribe a file" button and the
/// in-app record button). Runs off the UI thread via `Task::perform`; any
/// failure (no model configured, inference error) comes back as a string.
pub async fn run_transcribe_audio(audio: AudioBuffer, config: Config) -> Result<String, String> {
    let asr = build_asr_backend(&config)?;
    let refiner = build_refiner(&config)?;

    let pipeline = Pipeline::new(asr, refiner).with_language(config.language.clone());
    pipeline
        .run(audio, &RefineContext::default())
        .await
        .map_err(|e| e.to_string())
}
