//! Filesystem discovery of already-installed models: the whisper-only
//! single-directory [`installed`] scan (used by callers that only care about
//! ASR models in one dir).

use std::path::{Path, PathBuf};

use super::whisper::model_by_filename;

/// One model file found in a models directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModel {
    /// The file's name, e.g. `"ggml-base.bin"`.
    pub filename: String,
    /// Absolute path to the file (what `config.whisper.model_path` gets set
    /// to on "Use this model").
    pub path: PathBuf,
    /// Actual on-disk size in bytes.
    pub size_bytes: u64,
    /// The matching curated [`super::WhisperModel::id`], if this file is one
    /// of the known models; `None` for a `.bin` the user dropped in themselves.
    pub known_id: Option<&'static str>,
}

/// Scans `models_dir` for whisper GGML model files. Recognizes every curated
/// [`super::MODELS`] filename plus any other `ggml-*.bin` the user placed
/// there. Tolerates a missing directory (returns empty) since a fresh install
/// won't have downloaded anything yet. Results are sorted by filename for a
/// stable list order.
pub fn installed(models_dir: &Path) -> Vec<InstalledModel> {
    let Ok(entries) = std::fs::read_dir(models_dir) else {
        return Vec::new();
    };

    let mut found: Vec<InstalledModel> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            let is_model = model_by_filename(&name).is_some()
                || (name.starts_with("ggml-") && name.ends_with(".bin"));
            if !is_model {
                return None;
            }
            let size_bytes = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
            Some(InstalledModel {
                known_id: model_by_filename(&name).map(|m| m.id),
                filename: name,
                path,
                size_bytes,
            })
        })
        .collect();

    found.sort_by(|a, b| a.filename.cmp(&b.filename));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_scans_known_and_stray_bins_ignoring_others() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ggml-base.bin"), b"fake base model").unwrap();
        std::fs::write(dir.path().join("ggml-custom.bin"), b"user model").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"not a model").unwrap();

        let found = installed(dir.path());
        let names: Vec<&str> = found.iter().map(|m| m.filename.as_str()).collect();
        assert_eq!(names, vec!["ggml-base.bin", "ggml-custom.bin"]);

        let base = &found[0];
        assert_eq!(base.known_id, Some("base"));
        assert_eq!(base.size_bytes, "fake base model".len() as u64);

        // A stray ggml-*.bin is listed but not tied to a curated id.
        assert_eq!(found[1].known_id, None);
    }

    #[test]
    fn installed_tolerates_missing_dir() {
        assert!(installed(Path::new("/nonexistent/whspr-hf-models")).is_empty());
    }
}
