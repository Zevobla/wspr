//! The curated GGUF instruct-LLM registry used for text refinement, and its
//! `hf-hub`-backed [`download_llm`].
//!
//! Unlike the whisper models (all in one repo), each GGUF lives in its own
//! upstream repo, so every [`LlmModel`] carries its own [`LlmModel::repo`]. We
//! pin a single sensible quant (Q4_K_M) per model -- a good size/quality
//! tradeoff for a local refiner -- with an approximate on-disk size so the
//! GUI can show a size + "fits your machine" badge without a network call.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc::UnboundedSender;
use whspr_core::Result;

use super::{download_file, mib, DownloadProgress};

/// One curated GGUF refiner LLM: its stable id, the HuggingFace repo and file
/// to download, an approximate on-disk size (for the size + fit badge), and a
/// short human label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmModel {
    /// Stable identifier, e.g. `"qwen2.5-3b-instruct"`.
    pub id: &'static str,
    /// The HuggingFace repo the GGUF lives in, e.g.
    /// `"Qwen/Qwen2.5-3B-Instruct-GGUF"`.
    pub repo: &'static str,
    /// The GGUF file's name inside [`LlmModel::repo`].
    pub filename: &'static str,
    /// Approximate download/on-disk size in bytes (the Q4_K_M quant).
    pub size_bytes: u64,
    /// Short human label for the model list, e.g. `"Qwen2.5 3B Instruct"`.
    pub label: &'static str,
}

/// The curated GGUF refiner menu, smallest to largest. All entries are the
/// Q4_K_M quant. Sizes are approximate (see [`LlmModel::size_bytes`]).
pub const LLM_MODELS: &[LlmModel] = &[
    LlmModel {
        id: "qwen2.5-1.5b-instruct",
        repo: "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        size_bytes: mib(1120),
        label: "Qwen2.5 1.5B Instruct",
    },
    LlmModel {
        id: "llama-3.2-3b-instruct",
        repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        filename: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        size_bytes: mib(2020),
        label: "Llama 3.2 3B Instruct",
    },
    LlmModel {
        id: "qwen2.5-3b-instruct",
        repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",
        filename: "qwen2.5-3b-instruct-q4_k_m.gguf",
        size_bytes: mib(2100),
        label: "Qwen2.5 3B Instruct",
    },
    LlmModel {
        id: "phi-3.5-mini-instruct",
        repo: "bartowski/Phi-3.5-mini-instruct-GGUF",
        filename: "Phi-3.5-mini-instruct-Q4_K_M.gguf",
        size_bytes: mib(2390),
        label: "Phi-3.5 Mini Instruct",
    },
];

/// Looks up a curated LLM by its [`LlmModel::id`].
pub fn llm_model_by_id(id: &str) -> Option<&'static LlmModel> {
    LLM_MODELS.iter().find(|m| m.id == id)
}

/// Looks up a curated LLM by its [`LlmModel::filename`].
pub fn llm_model_by_filename(filename: &str) -> Option<&'static LlmModel> {
    LLM_MODELS.iter().find(|m| m.filename == filename)
}

/// Downloads `model` from its [`LlmModel::repo`] into `models_dir`, returning
/// the path to the flat `models_dir/<filename>` file. Thin wrapper over the
/// shared [`download_file`]; see that function for the token/progress
/// semantics.
pub async fn download_llm(
    model: &LlmModel,
    token: Option<String>,
    models_dir: &Path,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf> {
    download_file(model.repo, model.filename, token, models_dir, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_ids_and_filenames_are_unique() {
        for (i, a) in LLM_MODELS.iter().enumerate() {
            for b in &LLM_MODELS[i + 1..] {
                assert_ne!(a.id, b.id, "duplicate llm id {}", a.id);
                assert_ne!(a.filename, b.filename, "duplicate filename {}", a.filename);
            }
        }
    }

    #[test]
    fn every_llm_filename_is_a_gguf() {
        for m in LLM_MODELS {
            assert!(
                m.filename.to_ascii_lowercase().ends_with(".gguf"),
                "unexpected filename shape: {}",
                m.filename
            );
            assert!(!m.repo.is_empty(), "{} has an empty repo", m.id);
            assert!(m.size_bytes > 0, "{} has a zero size", m.id);
        }
    }

    #[test]
    fn llm_model_by_id_finds_and_misses() {
        assert_eq!(
            llm_model_by_id("qwen2.5-3b-instruct").unwrap().filename,
            "qwen2.5-3b-instruct-q4_k_m.gguf"
        );
        assert!(llm_model_by_id("does-not-exist").is_none());
    }

    #[test]
    fn llm_model_by_filename_roundtrips_with_by_id() {
        for m in LLM_MODELS {
            assert_eq!(llm_model_by_filename(m.filename).unwrap().id, m.id);
        }
    }
}
