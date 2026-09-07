//! The curated whisper.cpp GGML ASR model registry and its `hf-hub`-backed
//! [`download`].
//!
//! Every model lives in the one upstream repo ([`REPO`],
//! `ggerganov/whisper.cpp`); we hard-code a hand-picked subset (tiny .. large)
//! with each file's name and an approximate on-disk size so the GUI can show
//! a size + "fits your machine" badge without a network round-trip just to
//! populate the list.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc::UnboundedSender;
use whspr_core::Result;

use super::{download_file, mib, DownloadProgress};

/// The HuggingFace repo every curated whisper.cpp GGML model is fetched from.
pub const REPO: &str = "ggerganov/whisper.cpp";

/// One curated whisper.cpp GGML model: its stable id, the file to download
/// from [`REPO`], an approximate on-disk size (for the size + fit badge), and
/// a short human label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModel {
    /// Stable identifier, e.g. `"base.en"`. Matches whisper.cpp's own naming.
    pub id: &'static str,
    /// The GGML file's name inside [`REPO`], e.g. `"ggml-base.en.bin"`.
    pub filename: &'static str,
    /// Approximate download/on-disk size in bytes. Not exact -- good enough
    /// to size a progress estimate and pick a fit-badge color.
    pub size_bytes: u64,
    /// Short human label for the model list, e.g. `"Base (English)"`.
    pub label: &'static str,
}

/// The curated whisper.cpp model menu, smallest to largest. Sizes are
/// approximate (see [`WhisperModel::size_bytes`]).
pub const MODELS: &[WhisperModel] = &[
    WhisperModel {
        id: "tiny",
        filename: "ggml-tiny.bin",
        size_bytes: mib(75),
        label: "Tiny (multilingual)",
    },
    WhisperModel {
        id: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_bytes: mib(75),
        label: "Tiny (English)",
    },
    WhisperModel {
        id: "base",
        filename: "ggml-base.bin",
        size_bytes: mib(142),
        label: "Base (multilingual)",
    },
    WhisperModel {
        id: "base.en",
        filename: "ggml-base.en.bin",
        size_bytes: mib(142),
        label: "Base (English)",
    },
    WhisperModel {
        id: "small",
        filename: "ggml-small.bin",
        size_bytes: mib(466),
        label: "Small (multilingual)",
    },
    WhisperModel {
        id: "small.en",
        filename: "ggml-small.en.bin",
        size_bytes: mib(466),
        label: "Small (English)",
    },
    WhisperModel {
        id: "medium",
        filename: "ggml-medium.bin",
        size_bytes: mib(1500),
        label: "Medium (multilingual)",
    },
    WhisperModel {
        id: "medium.en",
        filename: "ggml-medium.en.bin",
        size_bytes: mib(1500),
        label: "Medium (English)",
    },
    WhisperModel {
        id: "large-v3",
        filename: "ggml-large-v3.bin",
        size_bytes: mib(3100),
        label: "Large v3 (multilingual)",
    },
    WhisperModel {
        id: "large-v3-turbo",
        filename: "ggml-large-v3-turbo.bin",
        size_bytes: mib(1620),
        label: "Large v3 Turbo (multilingual)",
    },
];

/// Looks up a curated model by its [`WhisperModel::id`].
pub fn model_by_id(id: &str) -> Option<&'static WhisperModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// Looks up a curated model by its [`WhisperModel::filename`].
pub fn model_by_filename(filename: &str) -> Option<&'static WhisperModel> {
    MODELS.iter().find(|m| m.filename == filename)
}

/// Downloads `model` from [`REPO`] into `models_dir`, returning the path to
/// the flat `models_dir/<filename>` file. Thin wrapper over the shared
/// [`download_file`]; see that function for the token/progress semantics.
pub async fn download(
    model: &WhisperModel,
    token: Option<String>,
    models_dir: &Path,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf> {
    download_file(REPO, model.filename, token, models_dir, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_are_unique() {
        for (i, a) in MODELS.iter().enumerate() {
            for b in &MODELS[i + 1..] {
                assert_ne!(a.id, b.id, "duplicate model id {}", a.id);
                assert_ne!(a.filename, b.filename, "duplicate filename {}", a.filename);
            }
        }
    }

    #[test]
    fn every_model_filename_is_a_ggml_bin() {
        for m in MODELS {
            assert!(
                m.filename.starts_with("ggml-") && m.filename.ends_with(".bin"),
                "unexpected filename shape: {}",
                m.filename
            );
            assert!(m.size_bytes > 0, "{} has a zero size", m.id);
        }
    }

    #[test]
    fn model_by_id_finds_and_misses() {
        assert_eq!(model_by_id("base.en").unwrap().filename, "ggml-base.en.bin");
        assert!(model_by_id("does-not-exist").is_none());
    }

    #[test]
    fn model_by_filename_roundtrips_with_model_by_id() {
        for m in MODELS {
            assert_eq!(model_by_filename(m.filename).unwrap().id, m.id);
        }
    }
}
