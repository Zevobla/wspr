//! The `whspr diarize` subcommand: builds a diarization backend, matches
//! resulting turns against the persisted speaker database, and prints the
//! result. Split out of `main.rs` to keep that file under this project's
//! 600-line-per-file guideline.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde_json::json;
use whspr_config::{SpeakerDb, SpeakerEmbeddingChoice};
use whspr_core::testkit::MockDiarizer;
use whspr_core::Diarizer;
use whspr_diarize::SherpaDiarizer;

/// Builds a diarization backend from config and command-line flags, falling
/// back to `MockDiarizer` when no model directory is available from
/// `--model-dir`, the config file's `[speaker].model_dir`, or the
/// `SPEAKER_MODEL_DIR` environment variable the project's Nix devShell/
/// package sets (see `SherpaDiarizer::resolve_model_dir`) -- mirrors
/// `main::build_asr_backend`'s "explicit opt-in, else a deterministic
/// default" reasoning: a real `SherpaDiarizer` needs model files that
/// aren't guaranteed present, so it's never constructed unless a model
/// directory is available from *some* source.
///
/// The embedding model itself is never hardcoded either: `--embedding`
/// (falling back to `config.speaker.embedding_model`) picks a
/// `SpeakerEmbeddingChoice`, which `SherpaDiarizer` resolves to a filename.
fn build_diarizer(
    config: &whspr_config::Config,
    model_dir_flag: Option<&Path>,
    embedding_flag: Option<&str>,
) -> anyhow::Result<Box<dyn Diarizer>> {
    let model_dir = SherpaDiarizer::resolve_model_dir(
        model_dir_flag
            .map(PathBuf::from)
            .or_else(|| config.speaker.model_dir.clone()),
    );

    match model_dir {
        Some(dir) => {
            let embedding_choice = match embedding_flag {
                Some(id) => {
                    SpeakerEmbeddingChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
                }
                None => config.speaker.embedding_model,
            };
            let diarizer = SherpaDiarizer::new(&dir, embedding_choice)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok(Box::new(diarizer))
        }
        None => Ok(Box::new(MockDiarizer::default())),
    }
}

/// Runs the `diarize` subcommand end to end: loads+resamples `file`, builds
/// a diarizer, matches every resulting turn against the persisted
/// `SpeakerDb`, saves it back, and prints the labeled turns (JSON or plain
/// text).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &whspr_config::Config,
    file: PathBuf,
    model_dir: Option<PathBuf>,
    embedding: Option<String>,
    data_dir: Option<PathBuf>,
    output_json: bool,
) -> anyhow::Result<()> {
    if !config.speaker.enabled {
        anyhow::bail!(
            "speaker fingerprinting is disabled ([speaker].enabled = false in config); \
             enable it to run `whspr diarize`"
        );
    }

    eprintln!("Loading audio...");
    let audio = crate::load_audio(&file).await?;
    let audio = whspr_audio::resample_to_16k_mono(&audio).map_err(|e| anyhow::anyhow!("{}", e))?;

    let diarizer = build_diarizer(config, model_dir.as_deref(), embedding.as_deref())?;

    eprintln!("Running diarization...");
    let turns = diarizer
        .diarize(&audio)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let data_dir = crate::resolve_data_dir(data_dir.as_deref())?;
    std::fs::create_dir_all(&data_dir)?;
    let speakers_path = data_dir.join("speakers.json");
    let mut speaker_db = SpeakerDb::load(&speakers_path);

    let scan_id = file.display().to_string();
    let threshold = config.speaker.similarity_threshold;
    let labeled_turns: Vec<_> = turns
        .into_iter()
        .map(|mut turn| {
            let (id, _is_new) = speaker_db.match_or_enroll(&turn.embedding, threshold, &scan_id);
            turn.speaker = Some(id);
            turn
        })
        .collect();

    speaker_db
        .save(&speakers_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if output_json {
        let json_out: Vec<_> = labeled_turns
            .iter()
            .map(|t| {
                json!({
                    "start_secs": t.start_secs,
                    "end_secs": t.end_secs,
                    "speaker": t.speaker,
                    "score": t.score,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&json_out)?);
    } else {
        for t in &labeled_turns {
            println!(
                "[{:.2}-{:.2}] {}",
                t.start_secs,
                t.end_secs,
                t.speaker.as_deref().unwrap_or("?")
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `[speaker].enabled = false` check runs before `file` is ever
    /// touched, so a nonexistent path is fine here: if this test somehow
    /// got past the check, `load_audio`'s file-not-found error would look
    /// nothing like the "disabled" error this test asserts on, so there's
    /// no risk of a false pass.
    #[tokio::test]
    async fn run_refuses_when_speaker_disabled() {
        let data_dir = tempfile::tempdir().expect("failed to create data dir");
        let mut config = whspr_config::Config::default();
        config.speaker.enabled = false;

        let err = run(
            &config,
            PathBuf::from("/nonexistent/whspr-diarize-cmd-test.wav"),
            None,
            None,
            Some(data_dir.path().to_path_buf()),
            false,
        )
        .await
        .expect_err("run should refuse when [speaker].enabled is false");

        assert!(
            err.to_string().contains("disabled"),
            "expected a disabled-feature error, got: {err}"
        );
    }
}
