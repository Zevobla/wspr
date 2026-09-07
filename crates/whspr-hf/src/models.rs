//! The curated whisper.cpp GGML model registry, a `hf-hub`-backed
//! [`download`], and an [`installed`] directory scan.
//!
//! Every model lives in the one upstream repo ([`REPO`],
//! `ggerganov/whisper.cpp`); we hard-code a hand-picked subset (tiny .. large)
//! with each file's name and an approximate on-disk size so the GUI can show
//! a size + "fits your machine" badge (see [`crate::hardware`]) *without* a
//! network round-trip just to populate the list.
//!
//! Downloaded models land as a plain `models_dir/<filename>` file (copied out
//! of hf-hub's nested blob cache) so [`installed`] can scan a flat directory
//! and `config.whisper.model_path` can point straight at the file -- the same
//! "one fixed directory of model files" shape `whspr-diarize` uses.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hf_hub::api::tokio::{ApiBuilder, Progress};
use tokio::sync::mpsc::UnboundedSender;
use whspr_core::{Result, WhsprError};

/// The HuggingFace repo every curated whisper.cpp GGML model is fetched from.
pub const REPO: &str = "ggerganov/whisper.cpp";

/// `n` mebibytes as a byte count, evaluated at compile time so [`MODELS`] can
/// stay a plain `const`.
const fn mib(n: u64) -> u64 {
    n * 1024 * 1024
}

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
    /// The matching curated [`WhisperModel::id`], if this file is one of the
    /// known models; `None` for a `.bin` the user dropped in themselves.
    pub known_id: Option<&'static str>,
}

/// Scans `models_dir` for whisper GGML model files. Recognizes every curated
/// [`MODELS`] filename plus any other `ggml-*.bin` the user placed there.
/// Tolerates a missing directory (returns empty) since a fresh install won't
/// have downloaded anything yet. Results are sorted by filename for a stable
/// list order.
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

/// A download progress update, forwarded over the channel [`download`] is
/// given. `downloaded` is monotonically non-decreasing and reaches `total`
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

/// Downloads `model` from [`REPO`] into `models_dir`, returning the path to
/// the flat `models_dir/<filename>` file (copied out of hf-hub's nested blob
/// cache). `token` is an optional HuggingFace access token (from the OAuth
/// flow or a saved config token) -- required only for gated/private repos,
/// `None` is fine for the public whisper.cpp models. If `progress` is
/// `Some`, byte-level [`DownloadProgress`] is streamed over it.
///
/// The network fetch runs on hf-hub's async client; the small blob-to-flat
/// copy is the only blocking bit. Callers run this off the UI thread (the GUI
/// via `Task::perform`).
pub async fn download(
    model: &WhisperModel,
    token: Option<String>,
    models_dir: &Path,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(models_dir)
        .map_err(|e| WhsprError::Other(format!("failed to create models dir: {e}")))?;

    // hf-hub caches into `<cache>/models--org--repo/{blobs,snapshots}/...`;
    // keep that machinery in a hidden subdir so the models dir itself stays a
    // flat list of usable `.bin` files.
    let cache_dir = models_dir.join(".hf-cache");
    let api = ApiBuilder::new()
        .with_progress(false)
        .with_cache_dir(cache_dir)
        .with_token(token)
        .build()
        .map_err(|e| WhsprError::Other(format!("failed to build HuggingFace client: {e}")))?;

    let repo = api.model(REPO.to_string());
    let cached = match progress {
        Some(tx) => {
            repo.download_with_progress(model.filename, ChannelProgress::new(tx))
                .await
        }
        None => repo.download(model.filename).await,
    }
    .map_err(|e| WhsprError::Other(format!("failed to download {}: {e}", model.filename)))?;

    let dest = models_dir.join(model.filename);
    std::fs::copy(&cached, &dest).map_err(|e| {
        WhsprError::Other(format!("failed to place model in {}: {e}", dest.display()))
    })?;
    Ok(dest)
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
