//! Reading and appending the history journal (`history.jsonl`) that the CLI
//! shares with the desktop app. When `[privacy].history_encryption` is on,
//! appended lines are encrypted with the keystore-held history key in the
//! same `enc1:` format the app writes (`whspr_config::history_codec`), and
//! readers accept plaintext and encrypted lines side by side.

use std::io::Write;
use std::path::Path;

use whspr_config::history_codec::{decode_line, encode_line, ENCRYPTED_PREFIX};
use whspr_config::{history_key, Config, Keystore, SecretName};

/// File name of the journal inside the data directory.
pub(crate) const HISTORY_FILE: &str = "history.jsonl";

/// Appends one entry to `data_dir/history.jsonl`.
///
/// With `[privacy].history_encryption` on, the line is encrypted. If the
/// history key can't be had then (no persistent keystore, a locked
/// keychain), nothing is written: the caller reports the error as a
/// warning, because falling back to plaintext would defeat the setting.
pub(crate) fn append_entry(
    data_dir: &Path,
    entry: &serde_json::Value,
    config: &Config,
    keystore: &dyn Keystore,
) -> anyhow::Result<()> {
    let json = entry.to_string();
    let line = if config.privacy.history_encryption {
        let key = history_key(keystore).map_err(|e| {
            anyhow::anyhow!(
                "history encryption is on but the history key is unavailable ({e}); entry not saved"
            )
        })?;
        encode_line(&json, &key)
    } else {
        json
    };
    std::fs::create_dir_all(data_dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_dir.join(HISTORY_FILE))?
        .write_all(format!("{line}\n").as_bytes())?;
    Ok(())
}

/// The journal's readable JSON lines, plus how many lines could not be
/// decoded (encrypted without an available key, or altered on disk).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct HistoryLines {
    pub(crate) json: Vec<String>,
    pub(crate) unreadable: usize,
}

/// Reads every non-blank line of the journal at `path`. A missing file is
/// an empty journal, not an error. The history key is looked up only when
/// the file actually holds encrypted lines, and is never generated here:
/// reading history must not create a keychain entry as a side effect.
pub(crate) fn read_lines(path: &Path, keystore: &dyn Keystore) -> anyhow::Result<HistoryLines> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HistoryLines::default()),
        Err(e) => return Err(e.into()),
    };
    let has_encrypted = contents
        .lines()
        .any(|line| line.trim_start().starts_with(ENCRYPTED_PREFIX));
    let key = if has_encrypted {
        existing_history_key(keystore)
    } else {
        None
    };
    let mut lines = HistoryLines::default();
    for line in contents.lines() {
        match decode_line(line, key.as_ref()) {
            Ok(Some(json)) => lines.json.push(json),
            Ok(None) => {}
            Err(_) => lines.unreadable += 1,
        }
    }
    Ok(lines)
}

/// The stored history key, or `None` when the keystore has none (or can't
/// be read). Checks for the entry first because `history_key` would
/// otherwise mint a fresh key.
fn existing_history_key(keystore: &dyn Keystore) -> Option<[u8; 32]> {
    match keystore.get(SecretName::HISTORY_KEY) {
        Ok(Some(_)) => history_key(keystore).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use whspr_config::MemoryKeystore;

    use super::*;

    fn encrypted_config() -> Config {
        let mut config = Config::default();
        config.privacy.history_encryption = true;
        config
    }

    #[test]
    fn plaintext_append_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let ks = MemoryKeystore::default();
        append_entry(dir.path(), &json!({"text": "hi"}), &Config::default(), &ks).unwrap();
        let lines = read_lines(&dir.path().join(HISTORY_FILE), &ks).unwrap();
        assert_eq!(lines.json, vec![r#"{"text":"hi"}"#.to_string()]);
        assert_eq!(lines.unreadable, 0);
    }

    #[test]
    fn encrypted_append_hides_the_text_and_reads_back_with_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let ks = MemoryKeystore::default();
        append_entry(
            dir.path(),
            &json!({"text": "secret words"}),
            &encrypted_config(),
            &ks,
        )
        .unwrap();
        let path = dir.path().join(HISTORY_FILE);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with(ENCRYPTED_PREFIX));
        assert!(!raw.contains("secret words"));
        let lines = read_lines(&path, &ks).unwrap();
        assert_eq!(lines.json, vec![r#"{"text":"secret words"}"#.to_string()]);
    }

    #[test]
    fn encryption_without_a_persistent_keystore_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ks = MemoryKeystore::non_persistent();
        let result = append_entry(dir.path(), &json!({"text": "x"}), &encrypted_config(), &ks);
        assert!(result.is_err());
        assert!(!dir.path().join(HISTORY_FILE).exists());
    }

    #[test]
    fn encrypted_lines_without_a_key_are_counted_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let writer = MemoryKeystore::default();
        append_entry(
            dir.path(),
            &json!({"text": "a"}),
            &Config::default(),
            &writer,
        )
        .unwrap();
        append_entry(
            dir.path(),
            &json!({"text": "b"}),
            &encrypted_config(),
            &writer,
        )
        .unwrap();
        let reader = MemoryKeystore::default();
        let lines = read_lines(&dir.path().join(HISTORY_FILE), &reader).unwrap();
        assert_eq!(lines.json, vec![r#"{"text":"a"}"#.to_string()]);
        assert_eq!(lines.unreadable, 1);
        assert_eq!(
            reader.get(SecretName::HISTORY_KEY).unwrap(),
            None,
            "reading must not mint a history key"
        );
    }

    #[test]
    fn missing_journal_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let lines = read_lines(&dir.path().join(HISTORY_FILE), &MemoryKeystore::default()).unwrap();
        assert_eq!(lines, HistoryLines::default());
    }
}
