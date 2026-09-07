//! The curated model registries (whisper.cpp GGML ASR models and instruct
//! GGUF refiner LLMs), a `hf-hub`-backed download, a magic-byte model-type
//! [`classify`], and a unified [`scan`] of the user's model directories.
//!
//! The crate keeps two hand-picked registries -- [`whisper::MODELS`] (ASR)
//! and [`llm::LLM_MODELS`] (refiner LLMs) -- each entry carrying the file to
//! download, its upstream HuggingFace repo, and an approximate on-disk size
//! so the GUI can show a size + "fits your machine" badge (see
//! [`crate::hardware`]) *without* a network round-trip just to populate a
//! list.
//!
//! Downloaded models land as a plain `<dir>/<filename>` file (copied out of
//! hf-hub's nested blob cache) so [`scan`] can walk a flat directory and the
//! relevant config path (`whisper.model_path` / `refine_settings.
//! llama_model_path`) can point straight at the file -- the same "one fixed
//! directory of model files" shape `whspr-diarize` uses.

mod llm;
mod scan;
mod whisper;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hf_hub::api::tokio::{ApiBuilder, Progress};
use tokio::sync::mpsc::UnboundedSender;
use whspr_core::{Result, WhsprError};

pub use llm::{download_llm, llm_model_by_filename, llm_model_by_id, LlmModel, LLM_MODELS};
pub use scan::{classify, installed, InstalledModel, ModelKind};
pub use whisper::{download, model_by_filename, model_by_id, WhisperModel, MODELS, REPO};

/// `n` mebibytes as a byte count, evaluated at compile time so the registries
/// can stay plain `const`s.
pub(crate) const fn mib(n: u64) -> u64 {
    n * 1024 * 1024
}

/// Formats a byte count as a short human string (`"142 MB"`, `"1.5 GB"`).
/// Uses mebi/gibibytes but labels them MB/GB, matching how model sizes are
/// colloquially quoted.
pub fn human_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("{} MB", bytes / MIB)
    }
}

/// Resolves which directory installed models live in: an explicit path (e.g.
/// from `[huggingface].models_dir`) wins; otherwise the `WHISPER_MODELS_DIR`
/// environment variable. Returns `None` if neither is set -- the caller (the
/// GUI) supplies a platform default as `explicit` in that case, mirroring
/// `whspr_diarize::SherpaDiarizer::resolve_model_dir`'s identical pattern
/// (explicit-or-env, no config/`directories` coupling in this leaf crate).
pub fn resolve_models_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(|| std::env::var_os("WHISPER_MODELS_DIR").map(PathBuf::from))
}

/// A download progress update, forwarded over the channel [`download_file`]
/// is given. `downloaded` is monotonically non-decreasing and reaches `total`
/// on completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Bytes fetched so far.
    pub downloaded: u64,
    /// Total bytes to fetch, per the server's `Content-Length`.
    pub total: u64,
}

/// A [`hf_hub`] [`Progress`] sink that reports byte counts over an mpsc
/// channel. hf-hub clones this once per parallel chunk, so the counters are
/// shared `Arc`s rather than per-clone locals -- every clone adds into the
/// same running total. Cloning the [`UnboundedSender`] is cheap and the sends
/// are non-blocking, so this never stalls the download.
#[derive(Clone)]
struct ChannelProgress {
    tx: UnboundedSender<DownloadProgress>,
    total: Arc<AtomicU64>,
    downloaded: Arc<AtomicU64>,
}

impl ChannelProgress {
    fn new(tx: UnboundedSender<DownloadProgress>) -> Self {
        Self {
            tx,
            total: Arc::new(AtomicU64::new(0)),
            downloaded: Arc::new(AtomicU64::new(0)),
        }
    }

    fn emit(&self) {
        // A closed receiver (GUI dropped the subscription) is fine -- the
        // download keeps going, we just stop reporting.
        let _ = self.tx.send(DownloadProgress {
            downloaded: self.downloaded.load(Ordering::SeqCst),
            total: self.total.load(Ordering::SeqCst),
        });
    }
}

impl Progress for ChannelProgress {
    async fn init(&mut self, size: usize, _filename: &str) {
        self.total.store(size as u64, Ordering::SeqCst);
        self.downloaded.store(0, Ordering::SeqCst);
        self.emit();
    }

    async fn update(&mut self, size: usize) {
        self.downloaded.fetch_add(size as u64, Ordering::SeqCst);
        self.emit();
    }

    async fn finish(&mut self) {
        // Pin downloaded to total so a subscriber always sees a clean 100%.
        self.downloaded
            .store(self.total.load(Ordering::SeqCst), Ordering::SeqCst);
        self.emit();
    }
}

/// Downloads `filename` from the HuggingFace `repo` into `models_dir`,
/// returning the path to the flat `models_dir/<filename>` file (copied out of
/// hf-hub's nested blob cache). `token` is an optional HuggingFace access
/// token (from the OAuth flow or a saved config token) -- required only for
/// gated/private repos, `None` is fine for the public whisper.cpp / GGUF
/// models. If `progress` is `Some`, byte-level [`DownloadProgress`] is
/// streamed over it.
///
/// The network fetch runs on hf-hub's async client; the small blob-to-flat
/// copy is the only blocking bit. Callers run this off the UI thread (the GUI
/// via `Task::perform`). Shared by [`download`] (whisper) and [`download_llm`].
pub(crate) async fn download_file(
    repo: &str,
    filename: &str,
    token: Option<String>,
    models_dir: &Path,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(models_dir)
        .map_err(|e| WhsprError::Other(format!("failed to create models dir: {e}")))?;

    // hf-hub caches into `<cache>/models--org--repo/{blobs,snapshots}/...`;
    // keep that machinery in a hidden subdir so the models dir itself stays a
    // flat list of usable model files.
    let cache_dir = models_dir.join(".hf-cache");
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(cache_dir)
        .with_token(token)
        .build()
        .map_err(|e| WhsprError::Other(format!("failed to build HuggingFace client: {e}")))?;

    let repo = api.model(repo.to_string());
    let cached = match progress {
        Some(tx) => {
            repo.download_with_progress(filename, ChannelProgress::new(tx))
                .await
        }
        None => repo.download(filename).await,
    }
    .map_err(|e| WhsprError::Other(format!("failed to download {filename}: {e}")))?;

    let dest = models_dir.join(filename);
    std::fs::copy(&cached, &dest).map_err(|e| {
        WhsprError::Other(format!("failed to place model in {}: {e}", dest.display()))
    })?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_uses_mb_below_a_gig_and_gb_above() {
        assert_eq!(human_size(mib(142)), "142 MB");
        assert_eq!(human_size(mib(1536)), "1.5 GB");
    }

    #[test]
    fn resolve_models_dir_precedence() {
        std::env::remove_var("WHISPER_MODELS_DIR");
        assert_eq!(resolve_models_dir(None), None);

        std::env::set_var("WHISPER_MODELS_DIR", "/from/env");
        assert_eq!(
            resolve_models_dir(None),
            Some(PathBuf::from("/from/env")),
            "should fall back to WHISPER_MODELS_DIR when no explicit dir is given"
        );
        assert_eq!(
            resolve_models_dir(Some(PathBuf::from("/explicit"))),
            Some(PathBuf::from("/explicit")),
            "an explicit dir should win over WHISPER_MODELS_DIR"
        );
        std::env::remove_var("WHISPER_MODELS_DIR");
    }

    #[tokio::test]
    async fn channel_progress_forwards_init_update_finish() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut progress = ChannelProgress::new(tx);

        progress.init(1000, "ggml-base.bin").await;
        assert_eq!(
            rx.recv().await.unwrap(),
            DownloadProgress {
                downloaded: 0,
                total: 1000
            }
        );

        progress.update(400).await;
        assert_eq!(
            rx.recv().await.unwrap(),
            DownloadProgress {
                downloaded: 400,
                total: 1000
            }
        );

        // A cloned sink (as hf-hub makes per chunk) adds into the same total.
        let mut chunk = progress.clone();
        chunk.update(600).await;
        assert_eq!(
            rx.recv().await.unwrap(),
            DownloadProgress {
                downloaded: 1000,
                total: 1000
            }
        );

        progress.finish().await;
        assert_eq!(
            rx.recv().await.unwrap(),
            DownloadProgress {
                downloaded: 1000,
                total: 1000
            }
        );
    }
}
