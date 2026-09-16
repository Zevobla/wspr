//! `[privacy].history_encryption` in the app: the key held while it is on,
//! how a new history entry reaches disk, switching the existing history file
//! between encrypted and plaintext in place, and loading history at boot.
//! The line format itself is `whspr_config::history_codec`'s.

use std::io::Write;
use std::path::Path;

use whspr_config::history_codec::{decode_line, encode_line};
use whspr_config::Keystore;
use whspr_core::WhsprError;

/// The 32-byte history key (`whspr_config::history_key`), held in memory
/// while history encryption is on. Its `Debug` output never shows the bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryKey([u8; 32]);

impl HistoryKey {
    /// The raw key, for `whspr_config::history_codec`.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
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
