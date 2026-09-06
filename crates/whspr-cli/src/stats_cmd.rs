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

/// T-09: wipes the stored history file so `whspr stats` starts fresh.
/// A file that doesn't exist yet isn't an error - there's nothing to
/// clear either way, which is a normal outcome, not a failure.
fn clear_history(history_path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(history_path) {
        Ok(()) => {
            println!("Cleared {}", history_path.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "No history to clear ({} does not exist)",
                history_path.display()
            );
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Per-(asr, refine)-pair totals for `--by-backend` (T-09): how many
/// utterances went through that pair, their average wpm, and total words.
#[derive(Default)]
struct BackendAggregate {
    count: usize,
    wpm_sum: f64,
    word_count_sum: usize,
}

/// T-09: groups entries by (asr, refine) backend pair instead of printing
/// one row per utterance. `BTreeMap` keeps the output in a stable,
/// deterministic order (alphabetical by asr then refine) rather than
/// history-file order.
fn print_by_backend(entries: &[HistoryEntry], csv: bool) {
    let mut groups: std::collections::BTreeMap<(String, String), BackendAggregate> =
        std::collections::BTreeMap::new();
    for e in entries {
        let agg = groups.entry((e.asr.clone(), e.refine.clone())).or_default();
        agg.count += 1;
        agg.wpm_sum += e.wpm;
        agg.word_count_sum += e.word_count;
    }

    if csv {
        println!("asr,refine,count,avg_wpm,total_words");
        for ((asr, refine), agg) in &groups {
            println!(
                "{},{},{},{:.1},{}",
                csv_field(asr),
                csv_field(refine),
                agg.count,
                agg.wpm_sum / agg.count as f64,
                agg.word_count_sum
            );
        }
    } else {
        for ((asr, refine), agg) in &groups {
            println!(
                "asr={}  refine={}  count={}  avg_wpm={:.0}  total_words={}",
                asr,
                refine,
                agg.count,
                agg.wpm_sum / agg.count as f64,
                agg.word_count_sum
            );
        }
    }
}

/// Runs the `stats` subcommand: reads every entry from `history.jsonl`
/// inside the resolved data dir and prints it either as CSV (`--csv`), a
/// per-backend breakdown (`--by-backend`), or a human-readable table -
/// or wipes the history entirely (`--clear`, which takes priority over
/// the other two since there'd be nothing left to print anyway).
pub async fn run(
    data_dir: Option<PathBuf>,
    csv: bool,
    clear: bool,
    by_backend: bool,
) -> anyhow::Result<()> {
    let data_dir = crate::resolve_data_dir(data_dir.as_deref())?;
    let history_path = data_dir.join("history.jsonl");

    if clear {
        return clear_history(&history_path);
    }

    let entries = load_entries(&history_path)?;

    if by_backend {
        print_by_backend(&entries, csv);
        return Ok(());
    }

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
