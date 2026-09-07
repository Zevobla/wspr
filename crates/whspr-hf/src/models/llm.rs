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
use crate::hardware::{estimated_llm_footprint, LlmShape};

/// One curated GGUF refiner LLM: its stable id, the HuggingFace repo and file
/// to download, an approximate on-disk size (for the size + fit badge), a
/// short human label, and the architecture metadata (from its HF model card
/// / `config.json`, cross-checked against the GGUF `<arch>.*` keys the same
/// values map to) [`LlmModel::estimated_footprint`] needs for an exact,
/// pre-download KV-cache estimate.
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
    /// Transformer block/layer count (GGUF `<arch>.block_count`).
    pub n_layers: u32,
    /// Hidden/embedding size (GGUF `<arch>.embedding_length`).
    pub n_embd: u32,
    /// Attention head count (GGUF `<arch>.attention.head_count`).
    pub n_head: u32,
    /// KV head count -- equal to [`Self::n_head`] for ordinary multi-head
    /// attention, smaller under grouped-query attention (GGUF
    /// `<arch>.attention.head_count_kv`).
    pub n_head_kv: u32,
    /// The model's trained/released context length (GGUF
    /// `<arch>.context_length`).
    pub context_length: u32,
}

impl LlmModel {
    /// This model's architecture shape, for
    /// [`crate::hardware::kv_cache_bytes`].
    pub fn shape(&self) -> LlmShape {
        LlmShape {
            n_layers: u64::from(self.n_layers),
            n_embd: u64::from(self.n_embd),
            n_head: u64::from(self.n_head),
            n_head_kv: u64::from(self.n_head_kv),
            context_length: u64::from(self.context_length),
        }
    }

    /// The exact, GGUF-metadata-based footprint estimate (weights, KV cache,
    /// and fixed overhead -- see [`crate::hardware::estimated_llm_footprint`]),
    /// available before download since every field it needs is baked into
    /// the curated registry rather than read from a downloaded file's
    /// header.
    pub fn estimated_footprint(&self) -> u64 {
        estimated_llm_footprint(self.size_bytes, self.shape())
    }
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
        // Qwen2.5-1.5B-Instruct config.json: hidden_size 1536,
        // num_hidden_layers 28, num_attention_heads 12,
        // num_key_value_heads 2 (GQA), max_position_embeddings 32768.
        n_layers: 28,
        n_embd: 1536,
        n_head: 12,
        n_head_kv: 2,
        context_length: 32768,
    },
    LlmModel {
        id: "llama-3.2-3b-instruct",
        repo: "bartowski/Llama-3.2-3B-Instruct-GGUF",
        filename: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        size_bytes: mib(2020),
        label: "Llama 3.2 3B Instruct",
        // Llama-3.2-3B-Instruct config.json: hidden_size 3072,
        // num_hidden_layers 28, num_attention_heads 24,
        // num_key_value_heads 8 (GQA), max_position_embeddings 131072.
        n_layers: 28,
        n_embd: 3072,
        n_head: 24,
        n_head_kv: 8,
        context_length: 131_072,
    },
    LlmModel {
        id: "qwen2.5-3b-instruct",
        repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",
        filename: "qwen2.5-3b-instruct-q4_k_m.gguf",
        size_bytes: mib(2100),
        label: "Qwen2.5 3B Instruct",
        // Qwen2.5-3B-Instruct config.json: hidden_size 2048,
        // num_hidden_layers 36, num_attention_heads 16,
        // num_key_value_heads 2 (GQA), max_position_embeddings 32768.
        n_layers: 36,
        n_embd: 2048,
        n_head: 16,
        n_head_kv: 2,
        context_length: 32768,
    },
    LlmModel {
        id: "phi-3.5-mini-instruct",
        repo: "bartowski/Phi-3.5-mini-instruct-GGUF",
        filename: "Phi-3.5-mini-instruct-Q4_K_M.gguf",
        size_bytes: mib(2390),
        label: "Phi-3.5 Mini Instruct",
        // Phi-3.5-mini-instruct config.json: hidden_size 3072,
        // num_hidden_layers 32, num_attention_heads 32,
        // num_key_value_heads 32 (no GQA -- ordinary multi-head attention),
        // max_position_embeddings 131072.
        n_layers: 32,
        n_embd: 3072,
        n_head: 32,
        n_head_kv: 32,
        context_length: 131_072,
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
