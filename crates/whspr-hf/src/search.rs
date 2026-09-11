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

use whspr_core::{Result, WhsprError};

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

/// Reads a JSON value as a `u64`, tolerating a float encoding (HF usually
/// sends plain integers, but a `12345.0` still yields `12345`) and defaulting
/// a missing/non-numeric field to `0`.
fn json_u64(value: &serde_json::Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_f64().map(|f| f as u64))
        .unwrap_or(0)
}

/// Parses the HuggingFace `/api/models` search response (a JSON array of repo
/// objects) into [`GgufRepoHit`]s, skipping any entry missing an `id`. Pure --
/// unit-tested against captured payloads, never hits the network.
pub fn parse_search_results(body: &str) -> Result<Vec<GgufRepoHit>> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| WhsprError::Other(format!("could not parse HF search response: {e}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| WhsprError::Other("HF search response was not a JSON array".to_string()))?;
    Ok(array
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(|v| v.as_str())?;
            Some(GgufRepoHit {
                id: id.to_string(),
                downloads: item.get("downloads").map(json_u64).unwrap_or(0),
                likes: item.get("likes").map(json_u64).unwrap_or(0),
            })
        })
        .collect())
}

/// Parses a HuggingFace repo tree response (`/tree/main?recursive=true`, a
/// JSON array of file/directory entries) into the `.gguf` files it contains,
/// including any nested in subfolders. Directories and non-GGUF files are
/// dropped; the result is sorted by path for a stable list order. Pure --
/// unit-tested, never hits the network.
pub fn parse_gguf_tree(body: &str) -> Result<Vec<GgufFile>> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| WhsprError::Other(format!("could not parse HF tree response: {e}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| WhsprError::Other("HF tree response was not a JSON array".to_string()))?;
    let mut files: Vec<GgufFile> = array
        .iter()
        .filter_map(|item| {
            let path = item.get("path").and_then(|v| v.as_str())?;
            if !path.to_ascii_lowercase().ends_with(".gguf") {
                return None;
            }
            Some(GgufFile {
                path: path.to_string(),
                size_bytes: item.get("size").map(json_u64).unwrap_or(0),
            })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
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

    const SEARCH_SAMPLE: &str = r#"[
      {"_id":"1","id":"bartowski/Llama-3.2-3B-Instruct-GGUF","likes":321,"downloads":98765,"tags":["gguf"]},
      {"_id":"2","id":"Qwen/Qwen2.5-3B-Instruct-GGUF","likes":210,"downloads":54321},
      {"_id":"3","likes":5,"downloads":10}
    ]"#;

    #[test]
    fn parse_search_results_reads_id_downloads_likes() {
        let hits = parse_search_results(SEARCH_SAMPLE).unwrap();
        // The third entry has no `id`, so it's skipped.
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "bartowski/Llama-3.2-3B-Instruct-GGUF");
        assert_eq!(hits[0].downloads, 98765);
        assert_eq!(hits[0].likes, 321);
        assert_eq!(hits[1].id, "Qwen/Qwen2.5-3B-Instruct-GGUF");
    }

    #[test]
    fn parse_search_results_defaults_missing_counters_to_zero() {
        let hits = parse_search_results(r#"[{"id":"org/repo-GGUF"}]"#).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].downloads, 0);
        assert_eq!(hits[0].likes, 0);
    }

    #[test]
    fn parse_search_results_rejects_non_array_json() {
        assert!(parse_search_results(r#"{"error":"nope"}"#).is_err());
        assert!(parse_search_results("not json at all").is_err());
    }

    const TREE_SAMPLE: &str = r#"[
      {"type":"directory","path":"assets","oid":"a"},
      {"type":"file","path":"README.md","size":1024,"oid":"b"},
      {"type":"file","path":"Llama-3.2-3B-Instruct-Q8_0.gguf","size":3421000000,"oid":"d"},
      {"type":"file","path":"Llama-3.2-3B-Instruct-Q4_K_M.gguf","size":2019377664,"oid":"c"},
      {"type":"file","path":"Q4_K_M/split-model.gguf","size":123456,"oid":"e"}
    ]"#;

    #[test]
    fn parse_gguf_tree_keeps_only_gguf_files_including_subfolders() {
        let files = parse_gguf_tree(TREE_SAMPLE).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        // README.md and the `assets` directory are dropped; the subfolder GGUF
        // is kept; the list is sorted by path.
        assert_eq!(
            paths,
            vec![
                "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
                "Llama-3.2-3B-Instruct-Q8_0.gguf",
                "Q4_K_M/split-model.gguf",
            ]
        );
        assert_eq!(files[0].size_bytes, 2_019_377_664);
    }

    #[test]
    fn parse_gguf_tree_is_case_insensitive_on_extension() {
        let files = parse_gguf_tree(r#"[{"type":"file","path":"Model.GGUF","size":7}]"#).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size_bytes, 7);
    }

    #[test]
    fn parse_gguf_tree_rejects_non_array_json() {
        assert!(parse_gguf_tree(r#"{"error":"gated"}"#).is_err());
    }
}
