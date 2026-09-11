//! Live HuggingFace GGUF model search for the refiner (LLM) picker: query the
//! public HF model index for community GGUF repos, list the `.gguf` files in a
//! chosen repo (with sizes), and download an arbitrary one into the models
//! directory through the same flat-file path the curated catalog uses -- so a
//! searched model then appears in the refiner selector like a curated one.
//!
//! Only the *refiner* side is searchable here: the curated whisper/GGML ASR
//! catalog is complete and GGML repos aren't cleanly filterable the way HF's
//! `filter=gguf` narrows to GGUF weight repos.
//!
//! # Network + auth
//!
//! The two GET calls mirror [`crate::oauth::whoami`]: they use oauth2's
//! re-exported `reqwest` (so this crate never depends on reqwest directly),
//! send the optional HuggingFace access token as an `Authorization: Bearer`
//! header (never logged), and map every transport/status/parse failure to a
//! `WhsprError` the GUI can show as a plain error line. A short timeout keeps
//! an offline or hung request from blocking the UI task forever.
//!
//! # Testability
//!
//! The HTTP edges (`search_gguf`, `list_gguf_files`) are thin wrappers over
//! pure JSON parsers (`parse_search_results`, `parse_gguf_tree`) which are
//! unit-tested against captured sample payloads -- the tests never touch the
//! network.

/// One repository hit from a GGUF model search: its `org/name` id and the
/// popularity counters the GUI sorts by / annotates with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufRepoHit {
    /// The repo id, e.g. `"bartowski/Llama-3.2-3B-Instruct-GGUF"`.
    pub id: String,
    /// All-time download count (HF `downloads`), `0` if the field was absent.
    pub downloads: u64,
    /// Like count (HF `likes`), `0` if absent.
    pub likes: u64,
}

/// One `.gguf` file inside a repo's tree: its repo-relative path (which may sit
/// in a subfolder) and on-disk size in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufFile {
    /// Repo-relative path, e.g. `"Llama-3.2-3B-Instruct-Q4_K_M.gguf"` or
    /// `"Q4_K_M/model.gguf"` when the quant lives in a subfolder.
    pub path: String,
    /// File size in bytes (HF tree `size`), `0` if the field was absent.
    pub size_bytes: u64,
}

impl GgufFile {
    /// The bare file name (last path component) -- what the downloaded file is
    /// stored as in the flat models directory, dropping any subfolder prefix.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gguf_file_name_drops_the_subfolder_prefix() {
        let nested = GgufFile {
            path: "Q4_K_M/model.gguf".to_string(),
            size_bytes: 1,
        };
        assert_eq!(nested.file_name(), "model.gguf");

        let flat = GgufFile {
            path: "flat.gguf".to_string(),
            size_bytes: 1,
        };
        assert_eq!(flat.file_name(), "flat.gguf");
    }
}
