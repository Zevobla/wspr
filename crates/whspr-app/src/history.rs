//! Tolerant history reading.
//!
//! whspr-app doesn't own the on-disk history file's schema (no other crate
//! has settled one yet), so this reads whatever's there defensively: any
//! line that isn't a JSON object with at least a string `"text"` field is
//! skipped rather than treated as an error. That keeps this forward
//! compatible with whatever shape another tool (e.g. whspr-cli) eventually
//! settles on, as long as it keeps a `text` field.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One completed transcription, either read from the on-disk history file
/// or appended in-memory as pipeline runs complete during this session.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub text: String,
    /// Recording duration in seconds, if known. `None` for lines that don't
    /// carry timing -- callers computing wpm should skip those rather than
    /// inventing a duration.
    pub duration_secs: Option<f32>,
}

impl HistoryEntry {
    /// Whitespace-separated word count of the transcription text.
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

/// Parses a JSONL history file's contents into entries, skipping any line
/// that isn't a JSON object with at least a string `"text"` field.
pub fn parse_history_jsonl(contents: &str) -> Vec<HistoryEntry> {
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|value| {
            let text = value.get("text")?.as_str()?.to_string();
            let duration_secs = value
                .get("duration_secs")
                .and_then(Value::as_f64)
                .map(|d| d as f32);
            Some(HistoryEntry {
                text,
                duration_secs,
            })
        })
        .collect()
}

/// The whspr history file's path in the platform data dir, if determinable
/// on this platform. Whether the file actually exists yet is a separate
/// question -- see `read_history_file`.
pub fn history_file_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "whspr")?;
    Some(dirs.data_dir().join("history.jsonl"))
}

/// Reads and parses the history file at `path`, tolerating a missing file
/// (returns empty, not an error) since a fresh install won't have one yet.
pub fn read_history_file(path: &Path) -> Vec<HistoryEntry> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_history_jsonl(&contents),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_lines() {
        let contents = "{\"text\": \"hello world\", \"duration_secs\": 2.0}\n\
                         {\"text\": \"a second entry\"}\n";

        let entries = parse_history_jsonl(contents);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "hello world");
        assert_eq!(entries[0].duration_secs, Some(2.0));
        assert_eq!(entries[1].text, "a second entry");
        assert_eq!(entries[1].duration_secs, None);
    }

    #[test]
    fn skips_malformed_and_schema_mismatched_lines() {
        let contents = "not json at all\n\
                         {\"no_text_field\": true}\n\
                         {\"text\": 42}\n\
                         \n\
                         {\"text\": \"the only valid line\"}\n";

        let entries = parse_history_jsonl(contents);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "the only valid line");
    }

    #[test]
    fn empty_contents_yields_empty_history() {
        assert!(parse_history_jsonl("").is_empty());
    }

    #[test]
    fn word_count_splits_on_whitespace() {
        let entry = HistoryEntry {
            text: "the quick brown fox".to_string(),
            duration_secs: None,
        };

        assert_eq!(entry.word_count(), 4);
    }

    #[test]
    fn read_history_file_returns_empty_for_missing_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let missing = dir.path().join("does-not-exist.jsonl");

        assert!(read_history_file(&missing).is_empty());
    }

    #[test]
    fn read_history_file_parses_an_existing_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path, "{\"text\": \"from disk\"}\n").expect("failed to write history file");

        let entries = read_history_file(&path);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "from disk");
    }
}
