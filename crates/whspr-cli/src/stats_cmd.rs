//! The `whspr stats` subcommand: reads the history journal
//! (`history.jsonl`, written by `main::save_to_history`) and prints
//! per-utterance statistics, optionally as CSV (AL-12: wpm + word count).
//! Split out of `main.rs` to keep that file under this project's
//! 600-line-per-file guideline.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use whspr_config::Keystore;

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

/// Reads and parses every readable line of `history_path` (plaintext or
/// encrypted -- see `crate::history_io::read_lines`). A missing file (no
/// utterances stored yet) is not an error. Returns the entries and how many
/// encrypted lines could not be decrypted; an unparsable *decoded* line
/// still fails the command, as before.
fn load_entries(
    history_path: &Path,
    keystore: &dyn Keystore,
) -> anyhow::Result<(Vec<HistoryEntry>, usize)> {
    let lines = crate::history_io::read_lines(history_path, keystore)?;
    let entries = lines
        .json
        .iter()
        .map(|line| serde_json::from_str(line).map_err(anyhow::Error::from))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok((entries, lines.unreadable))
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
    keystore: &dyn Keystore,
    data_dir: Option<PathBuf>,
    csv: bool,
    clear: bool,
    by_backend: bool,
) -> anyhow::Result<()> {
    let data_dir = crate::resolve_data_dir(data_dir.as_deref())?;
    let history_path = data_dir.join(crate::history_io::HISTORY_FILE);

    if clear {
        return clear_history(&history_path);
    }

    let (entries, unreadable) = load_entries(&history_path, keystore)?;
    if unreadable > 0 {
        eprintln!(
            "Warning: skipped {unreadable} encrypted history line(s) that could not be decrypted \
             (no history key in this keystore, or the line was altered)."
        );
    }

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
        let (entries, _) = load_entries(
            Path::new("/nonexistent/whspr-stats-test/history.jsonl"),
            &whspr_config::MemoryKeystore::default(),
        )
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

        let (entries, unreadable) =
            load_entries(&path, &whspr_config::MemoryKeystore::default()).unwrap();
        assert_eq!(unreadable, 0);
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
