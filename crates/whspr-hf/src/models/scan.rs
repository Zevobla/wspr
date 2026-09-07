//! Filesystem discovery of installed models: a magic-byte [`classify`] that
//! sorts a file into ASR (whisper GGML) vs LLM (GGUF), plus the whisper-only
//! single-directory [`installed`] scan (used by callers that only care about
//! ASR models in one dir).

use std::path::{Path, PathBuf};

use super::whisper::model_by_filename;

/// Which selector a discovered model file belongs to: an ASR (whisper GGML)
/// model or a text-refiner LLM (GGUF). Determined from the file itself (see
/// [`classify`]), never from a user-supplied tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// A whisper.cpp GGML speech-recognition model (the ASR selector).
    Asr,
    /// A GGUF instruct LLM used as a text refiner (the refiner selector).
    Llm,
}

/// Classifies the model file at `path` by reading its leading magic bytes,
/// falling back to the filename extension when the bytes are inconclusive
/// (e.g. an unreadable or truncated file). Returns `None` for anything that
/// is neither a GGUF LLM nor a whisper GGML model, so the caller can skip it.
///
/// - GGUF files start with the ASCII magic `GGUF` (`0x47 47 55 46`) -> [`ModelKind::Llm`].
/// - whisper.cpp GGML files start with the `ggml` magic (`0x67676d6c`),
///   which lands on disk as the bytes `ggml` or, byte-swapped, `lmgg`
///   -> [`ModelKind::Asr`].
/// - otherwise: a `*.gguf` extension -> LLM, a `ggml-*.bin` name -> ASR.
pub fn classify(path: &Path) -> Option<ModelKind> {
    classify_magic(&read_magic(path)).or_else(|| classify_name(path))
}

/// Reads up to the first 4 bytes of `path` for a magic-number check.
/// Tolerates any I/O error by returning an empty slice, so [`classify`] just
/// falls through to the filename heuristic rather than propagating the error.
fn read_magic(path: &Path) -> [u8; 4] {
    use std::io::Read;
    let mut buf = [0u8; 4];
    if let Ok(mut file) = std::fs::File::open(path) {
        // A short read (file smaller than 4 bytes) leaves the tail zeroed,
        // which matches none of the magics below -- exactly what we want.
        let _ = file.read(&mut buf);
    }
    buf
}

/// The magic-byte half of [`classify`]: matches the leading 4 bytes against
/// the GGUF and whisper GGML magics (both byte orders for the latter).
fn classify_magic(magic: &[u8]) -> Option<ModelKind> {
    match magic {
        b"GGUF" => Some(ModelKind::Llm),
        b"ggml" | b"lmgg" => Some(ModelKind::Asr),
        _ => None,
    }
}

/// The filename-extension fallback of [`classify`], for files whose magic was
/// unreadable: `*.gguf` is an LLM, a `ggml-*.bin` is a whisper ASR model.
fn classify_name(path: &Path) -> Option<ModelKind> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".gguf") {
        Some(ModelKind::Llm)
    } else if name.starts_with("ggml-") && name.ends_with(".bin") {
        Some(ModelKind::Asr)
    } else {
        None
    }
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

    #[test]
    fn classify_reads_gguf_magic_as_llm() {
        let dir = tempfile::tempdir().unwrap();
        // A `.bin` extension deliberately conflicts with the ASR heuristic to
        // prove the magic bytes win over the filename.
        let path = dir.path().join("mystery.bin");
        std::fs::write(&path, b"GGUF\0\0\0\0rest").unwrap();
        assert_eq!(classify(&path), Some(ModelKind::Llm));
    }

    #[test]
    fn classify_reads_ggml_magic_as_asr_both_byte_orders() {
        let dir = tempfile::tempdir().unwrap();
        for magic in [b"ggml", b"lmgg"] {
            let path = dir.path().join("model.dat");
            std::fs::write(&path, magic).unwrap();
            assert_eq!(classify(&path), Some(ModelKind::Asr), "magic {magic:?}");
        }
    }

    #[test]
    fn classify_falls_back_to_extension_when_magic_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let gguf = dir.path().join("qwen2.5-3b-instruct-q4_k_m.gguf");
        std::fs::write(&gguf, b"not-a-known-magic").unwrap();
        assert_eq!(classify(&gguf), Some(ModelKind::Llm));

        let ggml = dir.path().join("ggml-base.bin");
        std::fs::write(&ggml, b"junk").unwrap();
        assert_eq!(classify(&ggml), Some(ModelKind::Asr));
    }

    #[test]
    fn classify_rejects_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"just some text").unwrap();
        assert_eq!(classify(&path), None);
    }
}
