//! The `whspr stats` subcommand: reads the history journal
//! (`history.jsonl`, written by `main::save_to_history`) and prints
//! per-utterance statistics, optionally as CSV (AL-12: wpm + word count).
//! Split out of `main.rs` to keep that file under this project's
//! 600-line-per-file guideline.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One line of `history.jsonl`, as written by `main::save_to_history`.
/// Every field is `#[serde(default)]` so a line from an older/partial
/// history format still parses instead of failing the whole command over
/// one row.
#[derive(Debug, Deserialize)]
struct HistoryEntry {
    #[serde(default)]
    text: String,
    #[serde(default)]
    timestamp: u64,
    #[serde(default)]
    asr: String,
    #[serde(default)]
    refine: String,
    #[serde(default)]
    wpm: f64,
    #[serde(default)]
    word_count: usize,
}

/// Escapes one CSV field per RFC 4180: wraps in double quotes (doubling
/// any embedded quote) whenever the value contains a comma, quote, or
/// newline. No `csv` crate in the workspace deps - these are the only
/// columns this command emits, so a hand-rolled escaper is simpler than
/// pulling one in.
fn csv_field(value: &str) -> String {
    if value.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Reads and parses every non-blank line of `history_path`. A missing file
/// (e.g. no utterances stored yet) is not an error - it just means no
/// entries.
fn load_entries(history_path: &Path) -> anyhow::Result<Vec<HistoryEntry>> {
    if !history_path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(history_path)?;
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).map_err(anyhow::Error::from))
        .collect()
}

/// Runs the `stats` subcommand: reads every entry from `history.jsonl`
/// inside the resolved data dir and prints it either as CSV (`--csv`) or a
/// human-readable table.
pub async fn run(data_dir: Option<PathBuf>, csv: bool) -> anyhow::Result<()> {
    let data_dir = crate::resolve_data_dir(data_dir.as_deref())?;
    let entries = load_entries(&data_dir.join("history.jsonl"))?;

    if csv {
        println!("timestamp,asr,refine,wpm,word_count,text");
        for e in &entries {
            println!(
                "{},{},{},{},{},{}",
                e.timestamp,
                csv_field(&e.asr),
                csv_field(&e.refine),
                e.wpm,
                e.word_count,
                csv_field(&e.text),
            );
        }
    } else {
        for e in &entries {
            println!(
                "{}  wpm={:.0}  words={}  asr={}  refine={}",
                e.timestamp, e.wpm, e.word_count, e.asr, e.refine
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_entries_of_missing_file_is_empty() {
        let entries = load_entries(Path::new("/nonexistent/whspr-stats-test/history.jsonl"))
            .expect("a missing history file should not be an error");
        assert!(entries.is_empty());
    }

    #[test]
    fn load_entries_parses_lines_and_skips_blanks() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        std::fs::write(
            &path,
            "{\"text\":\"hello\",\"timestamp\":1,\"asr\":\"mock\",\"refine\":\"noop\",\"wpm\":120.0,\"word_count\":1}\n\
             \n\
             {\"text\":\"world\",\"timestamp\":2,\"asr\":\"mock\",\"refine\":\"noop\",\"wpm\":100.0,\"word_count\":1}\n",
        )
        .unwrap();

        let entries = load_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "hello");
        assert_eq!(entries[1].wpm, 100.0);
    }

    #[test]
    fn csv_field_quotes_only_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("line1\nline2"), "\"line1\nline2\"");
    }
}
