//! `[privacy].history_encryption` in the app: the key held while it is on,
//! how a new history entry reaches disk, switching the existing history file
//! between encrypted and plaintext in place, and loading history at boot.
//! The line format itself is `whspr_config::history_codec`'s.

use std::io::Write;
use std::path::Path;

use whspr_config::history_codec::{decode_line, encode_line};
use whspr_config::Keystore;
use whspr_core::WhsprError;

use crate::history::{history_file_path, read_history_file};
use crate::state::State;

/// The 32-byte history key (`whspr_config::history_key`), held in memory
/// while history encryption is on. Its `Debug` output never shows the bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryKey([u8; 32]);

impl HistoryKey {
    /// The raw key, for `whspr_config::history_codec`.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// A key with fixed bytes, for tests that exercise encrypted history
    /// without a keystore.
    #[cfg(test)]
    pub(crate) fn for_test(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl std::fmt::Debug for HistoryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HistoryKey(..)")
    }
}

/// Loads the history key from `keystore`, generating and storing it on first
/// use. Refused -- with a user-readable reason -- for a keystore that forgets
/// on reboot, since history encrypted under a lost key is lost too.
fn load_key(keystore: &dyn Keystore) -> Result<HistoryKey, String> {
    whspr_config::history_key(keystore)
        .map(HistoryKey)
        .map_err(|error| match error {
            WhsprError::Config(reason) => reason,
            other => other.to_string(),
        })
}

/// How the next history entry reaches disk.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HistoryWrite<'a> {
    /// Encryption is off: a plain JSON line, as always.
    Plain,
    /// Encryption is on: an `enc1:` line under this key.
    Encrypted(&'a [u8; 32]),
    /// Encryption is on but its key could not be loaded: keep the entry in
    /// memory for this session rather than write it in the clear.
    MemoryOnly,
}

/// Decides [`HistoryWrite`] from the setting and the key loaded for it.
pub(crate) fn history_write(encryption_on: bool, key: Option<&HistoryKey>) -> HistoryWrite<'_> {
    match (encryption_on, key) {
        (false, _) => HistoryWrite::Plain,
        (true, Some(key)) => HistoryWrite::Encrypted(key.bytes()),
        (true, None) => HistoryWrite::MemoryOnly,
    }
}

/// Which way [`rewrite_history_file`] converts the history file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rewrite {
    /// Every readable line becomes an `enc1:` line.
    Encrypt,
    /// Every readable line becomes plain JSON again.
    Decrypt,
}

/// Converts the history file at `path` so every readable line is encrypted
/// under `key` (or plaintext), returning how many lines could not be
/// decrypted -- those are kept exactly as they were, never dropped. Blank
/// lines are dropped; a missing file is nothing to convert.
///
/// Atomic: the new contents are written and synced to a sibling temp file
/// that then replaces the original, so an interruption leaves either the
/// old file or the new one, never a half-written mix.
pub(crate) fn rewrite_history_file(
    path: &Path,
    key: &[u8; 32],
    direction: Rewrite,
) -> std::io::Result<usize> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut rewritten = String::with_capacity(contents.len() * 2);
    let mut unreadable = 0;
    for line in contents.lines() {
        let converted = match decode_line(line, Some(key)) {
            Ok(None) => continue,
            Ok(Some(json)) => match direction {
                Rewrite::Encrypt => encode_line(&json, key),
                Rewrite::Decrypt => json,
            },
            Err(_) => {
                unreadable += 1;
                line.to_string()
            }
        };
        rewritten.push_str(&converted);
        rewritten.push('\n');
    }

    let temp = path.with_extension("jsonl.rewrite");
    let replaced = write_synced(&temp, &rewritten).and_then(|()| std::fs::rename(&temp, path));
    if replaced.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    replaced.map(|()| unreadable)
}

/// Writes `contents` to a new file at `path` and flushes it to disk.
fn write_synced(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

/// Turns history encryption on or off for the running app, first converting
/// the history file at `path` to match (see [`rewrite_history_file`]).
/// Returns whether `state.config` changed and should be saved.
///
/// Nothing changes when the key can't be loaded -- notably a keystore that
/// does not survive a reboot, which keeps encryption off -- or the file
/// can't be rewritten; `state.history_note` then says why. After a
/// successful switch the note reports any lines left as they were.
pub(crate) fn set_history_encryption_at(
    state: &mut State,
    enabled: bool,
    path: Option<&Path>,
) -> bool {
    if state.config.privacy.history_encryption == enabled {
        return false;
    }
    let key = match state.history_key.clone() {
        Some(key) => key,
        None => match load_key(state.keystore.keystore()) {
            Ok(key) => key,
            Err(reason) => {
                let stays = if enabled { "unencrypted" } else { "encrypted" };
                state.history_note = Some(format!("History stays {stays}: {reason}."));
                return false;
            }
        },
    };
    let direction = if enabled {
        Rewrite::Encrypt
    } else {
        Rewrite::Decrypt
    };
    let unreadable = match path.map(|path| rewrite_history_file(path, key.bytes(), direction)) {
        None => 0,
        Some(Ok(unreadable)) => unreadable,
        Some(Err(error)) => {
            state.history_note = Some(format!("Could not rewrite the history file: {error}."));
            return false;
        }
    };
    state.config.privacy.history_encryption = enabled;
    state.history_key = enabled.then_some(key);
    state.history_note = (unreadable > 0).then(|| {
        format!("{unreadable} history entries could not be decrypted and were left as they were.")
    });
    true
}

/// [`set_history_encryption_at`] on the real history file.
pub(crate) fn set_history_encryption(state: &mut State, enabled: bool) -> bool {
    set_history_encryption_at(state, enabled, history_file_path().as_deref())
}

/// Boot step: loads the history key when encryption is on, then
/// `state.history` from the file at `path`, decrypting as it goes. A key
/// that can't be loaded is explained in `state.history_note` (new entries
/// then stay in memory -- see [`history_write`]); lines that can't be
/// decrypted are skipped and reported once through `state.notice`.
pub(crate) fn load_history_at(state: &mut State, path: Option<&Path>) {
    if state.config.privacy.history_encryption {
        match load_key(state.keystore.keystore()) {
            Ok(key) => state.history_key = Some(key),
            Err(reason) => {
                state.history_note = Some(format!(
                    "History encryption is on but its key could not be loaded ({reason}); \
                     new dictations are kept for this session only."
                ));
            }
        }
    }
    let key = state.history_key.as_ref().map(HistoryKey::bytes);
    let read = path
        .map(|path| read_history_file(path, key))
        .unwrap_or_default();
    state.history = read.entries;
    if read.unreadable > 0 {
        state.notice = Some(format!(
            "{} history entries could not be decrypted and were skipped.",
            read.unreadable
        ));
    }
}

/// [`load_history_at`] on the real history file.
pub(crate) fn load_history(state: &mut State) {
    load_history_at(state, history_file_path().as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_writes_follow_the_setting_and_the_loaded_key() {
        let key = HistoryKey([1; 32]);
        assert_eq!(history_write(false, None), HistoryWrite::Plain);
        assert_eq!(history_write(false, Some(&key)), HistoryWrite::Plain);
        assert_eq!(
            history_write(true, Some(&key)),
            HistoryWrite::Encrypted(&[1; 32])
        );
        assert_eq!(history_write(true, None), HistoryWrite::MemoryOnly);
    }

    #[test]
    fn history_key_debug_output_hides_the_bytes() {
        assert_eq!(format!("{:?}", HistoryKey([42; 32])), "HistoryKey(..)");
    }

    const KEY: [u8; 32] = [6; 32];

    fn history_file(dir: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path, contents).expect("failed to write history file");
        path
    }

    #[test]
    fn encrypting_then_decrypting_restores_every_entry() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = history_file(&dir, "{\"text\":\"one\"}\n\n{\"text\":\"two\"}\n");

        assert_eq!(
            rewrite_history_file(&path, &KEY, Rewrite::Encrypt).unwrap(),
            0
        );
        let encrypted = std::fs::read_to_string(&path).unwrap();
        assert!(encrypted.lines().all(|line| line.starts_with("enc1:")));
        assert!(!encrypted.contains("one"));

        assert_eq!(
            rewrite_history_file(&path, &KEY, Rewrite::Decrypt).unwrap(),
            0
        );
        let decrypted = std::fs::read_to_string(&path).unwrap();
        assert_eq!(decrypted, "{\"text\":\"one\"}\n{\"text\":\"two\"}\n");
        assert!(!path.with_extension("jsonl.rewrite").exists());
    }

    #[test]
    fn undecryptable_lines_are_kept_verbatim_and_counted() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let foreign = encode_line("{\"text\":\"other key\"}", &[9; 32]);
        let path = history_file(&dir, &format!("{foreign}\n{{\"text\":\"mine\"}}\n"));

        assert_eq!(
            rewrite_history_file(&path, &KEY, Rewrite::Decrypt).unwrap(),
            1
        );

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, format!("{foreign}\n{{\"text\":\"mine\"}}\n"));
    }

    #[test]
    fn a_missing_history_file_is_nothing_to_rewrite() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("history.jsonl");
        assert_eq!(
            rewrite_history_file(&path, &KEY, Rewrite::Encrypt).unwrap(),
            0
        );
        assert!(!path.exists());
    }

    fn state_with(keystore: whspr_config::MemoryKeystore) -> State {
        State::with_keystore(
            whspr_config::Config::default(),
            crate::secret_store::SecretStore::new(std::sync::Arc::new(keystore)),
        )
    }

    #[test]
    fn turning_encryption_on_then_off_converts_the_file_both_ways() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = history_file(&dir, "{\"text\":\"dictated\"}\n");
        let mut state = state_with(whspr_config::MemoryKeystore::default());

        assert!(set_history_encryption_at(&mut state, true, Some(&path)));
        assert!(state.config.privacy.history_encryption);
        assert!(state.history_key.is_some());
        assert!(!std::fs::read_to_string(&path).unwrap().contains("dictated"));

        assert!(set_history_encryption_at(&mut state, false, Some(&path)));
        assert!(!state.config.privacy.history_encryption);
        assert!(state.history_key.is_none());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"text\":\"dictated\"}\n"
        );
        assert_eq!(state.history_note, None);
    }

    #[test]
    fn a_non_persistent_keystore_keeps_encryption_off_and_says_why() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = history_file(&dir, "{\"text\":\"dictated\"}\n");
        let mut state = state_with(whspr_config::MemoryKeystore::non_persistent());

        assert!(!set_history_encryption_at(&mut state, true, Some(&path)));

        assert!(!state.config.privacy.history_encryption);
        assert!(state.history_key.is_none());
        let note = state.history_note.expect("the refusal is explained");
        assert!(note.starts_with("History stays unencrypted"), "{note}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"text\":\"dictated\"}\n"
        );
    }

    #[test]
    fn boot_loads_encrypted_history_and_reports_skipped_lines() {
        let keystore = whspr_config::MemoryKeystore::default();
        let key = whspr_config::history_key(&keystore).expect("persistent keystore");
        let foreign = encode_line("{\"text\":\"other key\"}", &[9; 32]);
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = history_file(
            &dir,
            &format!(
                "{}\n{{\"text\":\"plain\"}}\n{foreign}\n",
                encode_line("{\"text\":\"secret\"}", &key)
            ),
        );
        let mut state = state_with(keystore);
        state.config.privacy.history_encryption = true;

        load_history_at(&mut state, Some(&path));

        let texts: Vec<&str> = state.history.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["secret", "plain"]);
        assert_eq!(
            state.history_key.as_ref().map(HistoryKey::bytes),
            Some(&key)
        );
        assert_eq!(
            state.notice.as_deref(),
            Some("1 history entries could not be decrypted and were skipped.")
        );
    }

    #[test]
    fn boot_without_a_usable_key_explains_and_keeps_new_entries_in_memory() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = history_file(&dir, "{\"text\":\"plain\"}\n");
        let mut state = state_with(whspr_config::MemoryKeystore::non_persistent());
        state.config.privacy.history_encryption = true;

        load_history_at(&mut state, Some(&path));

        assert_eq!(state.history.len(), 1);
        assert!(state.history_key.is_none());
        assert!(state.history_note.is_some());
    }
}
