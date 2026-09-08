//! Tolerant history reading.
//!
//! whspr-app doesn't own the on-disk history file's schema (no other crate
//! has settled one yet), so this reads whatever's there defensively: any
//! line that isn't a JSON object with at least a string `"text"` field is
//! skipped rather than treated as an error. That keeps this forward
//! compatible with whatever shape another tool (e.g. whspr-cli) eventually
//! settles on, as long as it keeps a `text` field.

use std::io::Write;
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
    /// The attributed speaker's UUID (`whspr_config::SpeakerProfile::id`),
    /// if speaker attribution resolved one for this dictation. `None` when
    /// unresolved -- speaker fingerprinting disabled, no model installed, or
    /// the embedding failed (see `crate::speakers::attribute_speaker`).
    /// Serialized as the JSON line's `"speaker"` field (omitted when `None`).
    pub speaker_id: Option<String>,
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
            // Tolerant like every other field here: a missing (or non-string)
            // `"speaker"` reads back as `None` rather than skipping the line.
            let speaker_id = value
                .get("speaker")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(HistoryEntry {
                text,
                duration_secs,
                speaker_id,
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

/// Appends `entry` as one JSON line to the history file at `path`, creating
/// the file (and its parent data dir, on a fresh install) if this is the
/// first write. Field names match whspr-cli's own `save_to_history`
/// (`crates/whspr-cli/src/transcribe_cmd.rs`) so both tools keep reading
/// the same file as one format rather than two -- extra fields either
/// reader doesn't recognize are simply ignored (see this module's doc
/// comment and `stats_cmd.rs`'s `#[serde(default)]` fields).
fn append_history_entry(path: &Path, entry: &HistoryEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut line = serde_json::json!({
        "text": entry.text,
        "duration_secs": entry.duration_secs,
        "timestamp": timestamp,
        "source": "app",
    });
    // Only write `"speaker"` when a speaker was actually attributed, so
    // unattributed lines stay identical to the pre-speaker format rather
    // than carrying a null (`parse_history_jsonl` tolerates either).
    if let Some(speaker_id) = &entry.speaker_id {
        line["speaker"] = serde_json::Value::String(speaker_id.clone());
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// Adds one completed transcription to history: pushes it into
/// `state.history` (so RECENT and the History screen update immediately)
/// and appends it to the on-disk JSONL file at the real platform path (so
/// it survives a restart) -- used by the record-button/file-transcribe
/// completion path, which previously did neither (see `crate::app`'s
/// `Message::FileTranscribed` arm). Delegates to [`record_completed_at`],
/// which takes the path explicitly so tests can exercise the disk-write
/// behavior against a tempdir instead of the user's real history file.
pub fn record_completed(
    state: &mut crate::state::State,
    text: String,
    duration_secs: Option<f32>,
    speaker_id: Option<String>,
) {
    record_completed_at(
        state,
        text,
        duration_secs,
        speaker_id,
        history_file_path().as_deref(),
    );
}

/// [`record_completed`]'s logic, writing to `path` (or skipping the disk
/// write entirely if `None`, e.g. the platform data dir couldn't be
/// determined) instead of always resolving the real platform history file.
/// A blank/whitespace-only transcript (e.g. silence) is skipped entirely,
/// in memory and on disk, rather than adding an empty row. A write failure
/// is logged, not fatal -- the entry still lands in `state.history` so the
/// session doesn't lose it.
fn record_completed_at(
    state: &mut crate::state::State,
    text: String,
    duration_secs: Option<f32>,
    speaker_id: Option<String>,
    path: Option<&Path>,
) {
    if text.trim().is_empty() {
        return;
    }
    let entry = HistoryEntry {
        text,
        duration_secs,
        speaker_id,
    };
    if let Some(path) = path {
        if let Err(e) = append_history_entry(path, &entry) {
            eprintln!("whspr: failed to save history entry: {e}");
        }
    }
    state.history.push(entry);
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
            speaker_id: None,
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

    #[test]
    fn append_history_entry_round_trips_through_read_history_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("nested").join("history.jsonl");
        let entry = HistoryEntry {
            text: "hello from the app".to_string(),
            duration_secs: Some(1.5),
            speaker_id: None,
        };

        append_history_entry(&path, &entry).expect("append should create the file and its parent");
        append_history_entry(&path, &entry).expect("a second append should append, not overwrite");

        let entries = read_history_file(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "hello from the app");
        assert_eq!(entries[0].duration_secs, Some(1.5));
    }

    /// A `speaker_id` survives the on-disk JSONL round trip via the line's
    /// `"speaker"` field, and an unattributed entry reads back as `None`.
    #[test]
    fn speaker_id_round_trips_through_disk_as_the_speaker_field() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        let attributed = HistoryEntry {
            text: "attributed line".to_string(),
            duration_secs: Some(2.0),
            speaker_id: Some("spk-uuid-123".to_string()),
        };
        let unattributed = HistoryEntry {
            text: "unattributed line".to_string(),
            duration_secs: None,
            speaker_id: None,
        };

        append_history_entry(&path, &attributed).expect("append should succeed");
        append_history_entry(&path, &unattributed).expect("append should succeed");

        // The raw line carries `"speaker"` only for the attributed entry.
        let raw = std::fs::read_to_string(&path).expect("history file should exist");
        assert!(raw.contains("\"speaker\":\"spk-uuid-123\""));

        let entries = read_history_file(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].speaker_id, Some("spk-uuid-123".to_string()));
        assert_eq!(entries[1].speaker_id, None);
    }

    /// The record/file-transcribe completion path (`crate::app`'s
    /// `Message::FileTranscribed` arm) must push into `state.history` *and*
    /// persist to disk -- this is the fix for the bug where dictating via
    /// the Record button never showed up in RECENT/History.
    #[test]
    fn record_completed_at_appends_to_state_history_and_disk() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        let mut state = crate::state::State::new(whspr_config::Config::default());

        record_completed_at(
            &mut state,
            "a real transcript".to_string(),
            Some(3.0),
            Some("spk-abc".to_string()),
            Some(&path),
        );

        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].text, "a real transcript");
        assert_eq!(state.history[0].duration_secs, Some(3.0));
        assert_eq!(state.history[0].speaker_id, Some("spk-abc".to_string()));

        let on_disk = read_history_file(&path);
        assert_eq!(on_disk.len(), 1);
        assert_eq!(on_disk[0].text, "a real transcript");
        assert_eq!(on_disk[0].speaker_id, Some("spk-abc".to_string()));
    }

    /// A blank/silent transcript must not add a phantom history row, in
    /// memory or on disk.
    #[test]
    fn record_completed_at_skips_blank_transcripts() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        let mut state = crate::state::State::new(whspr_config::Config::default());

        record_completed_at(&mut state, "   ".to_string(), None, None, Some(&path));

        assert!(state.history.is_empty());
        assert!(!path.exists());
    }
}
