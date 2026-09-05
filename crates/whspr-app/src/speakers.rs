//! Speaker-enrollment persistence and the background diarization task for
//! the Hub's Speakers section. Mirrors `crate::history`'s data-dir pattern
//! (see that module) but for `whspr_config::SpeakerDb` instead of the
//! history JSONL.

use std::path::PathBuf;

use whspr_config::{SpeakerDb, SpeakerEmbeddingChoice};
use whspr_core::Diarizer;

/// The whspr speaker-database file's path in the platform data dir, if
/// determinable on this platform. Mirrors `crate::history::history_file_path`.
pub fn speaker_db_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "whspr")?;
    Some(dirs.data_dir().join("speakers.json"))
}

/// Decodes + resamples `file`, runs it through a `Diarizer` (a real
/// `SherpaDiarizer` if a model directory is available -- from `model_dir`,
/// or else the `SPEAKER_MODEL_DIR` env var the Nix devShell/package sets,
/// see `SherpaDiarizer::resolve_model_dir` -- otherwise the deterministic
/// `MockDiarizer`, same "explicit opt-in, else a safe default" philosophy
/// as `whspr-cli`'s backend builders), matches every resulting turn
/// against `speaker_db`, persists the updated db to `db_path`, and returns
/// the updated db plus how many turns were found. Runs entirely on a
/// blocking thread since decoding, resampling, and diarization are all
/// synchronous/CPU-bound.
pub async fn run_diarize_scan(
    file: PathBuf,
    model_dir: Option<PathBuf>,
    embedding_choice: SpeakerEmbeddingChoice,
    threshold: f32,
    mut speaker_db: SpeakerDb,
    db_path: PathBuf,
) -> Result<(SpeakerDb, usize), String> {
    tokio::task::spawn_blocking(move || {
        let audio = whspr_audio::decode_wav(&file).map_err(|e| e.to_string())?;
        let audio = whspr_audio::resample_to_16k_mono(&audio).map_err(|e| e.to_string())?;

        let diarizer: Box<dyn Diarizer> =
            match whspr_diarize::SherpaDiarizer::resolve_model_dir(model_dir) {
                Some(dir) => Box::new(
                    whspr_diarize::SherpaDiarizer::new(dir, embedding_choice)
                        .map_err(|e| e.to_string())?,
                ),
                None => Box::new(whspr_core::testkit::MockDiarizer::default()),
            };

        let turns = diarizer.diarize(&audio).map_err(|e| e.to_string())?;
        let scan_id = file.display().to_string();
        let count = turns.len();
        for turn in &turns {
            speaker_db.match_or_enroll(&turn.embedding, threshold, &scan_id);
        }
        speaker_db.save(&db_path).map_err(|e| e.to_string())?;

        Ok((speaker_db, count))
    })
    .await
    .map_err(|e| format!("diarize task panicked: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a minimal silent WAV file to `path` so `run_diarize_scan` has
    /// something real to decode. Mirrors `whspr-cli`'s e2e test fixture
    /// helper (`create_test_wav` in `crates/whspr-cli/tests/e2e.rs`).
    fn write_test_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("failed to create test wav");
        for _ in 0..16_000 {
            writer.write_sample(0i16).expect("failed to write sample");
        }
        writer.finalize().expect("failed to finalize test wav");
    }

    #[tokio::test]
    async fn run_diarize_scan_against_mock_diarizer() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let wav_path = dir.path().join("recording.wav");
        write_test_wav(&wav_path);
        let db_path = dir.path().join("speakers.json");

        // `MockDiarizer::default()`'s two canned embeddings are orthogonal
        // (cosine similarity 0.0), so the real default similarity threshold
        // (`SpeakerSettings::default().similarity_threshold`, 0.7) is enough
        // to exercise two *distinct* speakers being enrolled, as intended.
        let result = run_diarize_scan(
            wav_path,
            None,
            SpeakerEmbeddingChoice::default(),
            0.7,
            SpeakerDb::default(),
            db_path.clone(),
        )
        .await
        .expect("run_diarize_scan should succeed with the mock diarizer");

        let (db, count) = result;
        assert_eq!(count, 2);
        assert_eq!(db.profiles.len(), 2);
        assert!(db_path.is_file());
    }
}
