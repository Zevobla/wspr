//! The random symmetric key backing `[privacy].history_encryption` (see
//! `PrivacySettings::history_encryption`). whspr-config only produces and
//! stores this key -- encrypting/decrypting the history file with it is
//! the consuming crate's job.

use chacha20poly1305::aead::OsRng;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};

use crate::secrets::{Keystore, SecretName};

/// Returns the history-encryption key: the existing
/// [`SecretName::HISTORY_KEY`] entry in `ks` if one is already there, or a
/// freshly generated random 256-bit key -- via `ChaCha20Poly1305`'s
/// `OsRng`-backed `KeyInit::generate_key`, i.e. real OS randomness, not a
/// hand-rolled RNG -- stored there and returned, if not.
///
/// [`Keystore`] is a string store, so the 32 bytes are hex-encoded for
/// storage and decoded back on the way out (no extra dependency needed for
/// that -- see [`encode_hex`]/[`decode_hex`]).
///
/// Stable across calls for a given keystore: once generated, the same 32
/// bytes come back every time until something deletes the entry.
///
/// Refuses (`Err`) when [`Keystore::is_persistent`] is `false`: a key that
/// vanishes on reboot would make every history entry encrypted under it
/// permanently unreadable, so history encryption must stay off there.
pub fn history_key(ks: &dyn Keystore) -> whspr_core::Result<[u8; 32]> {
    if !ks.is_persistent() {
        return Err(whspr_core::WhsprError::Config(
            "history encryption needs a keystore that survives a reboot, \
             and this platform's does not"
                .to_string(),
        ));
    }
    if let Some(existing) = ks.get(SecretName::HISTORY_KEY)? {
        return decode_hex(&existing);
    }

    let key = ChaCha20Poly1305::generate_key(&mut OsRng);
    let bytes: [u8; 32] = key.into();
    ks.set(SecretName::HISTORY_KEY, &encode_hex(&bytes))?;
    Ok(bytes)
}

/// Lowercase-hex-encodes `bytes` (64 chars for a 32-byte key).
fn encode_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The inverse of [`encode_hex`]. Errors (as `WhsprError::Config`) on
/// anything that isn't exactly 64 hex characters -- a corrupted or
/// hand-edited keystore entry should fail loudly, not silently truncate or
/// panic.
fn decode_hex(hex: &str) -> whspr_core::Result<[u8; 32]> {
    if hex.len() != 64 {
        return Err(whspr_core::WhsprError::Config(format!(
            "history key: expected 64 hex characters, got {}",
            hex.len()
        )));
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| {
            whspr_core::WhsprError::Config(format!("history key: invalid hex: {e}"))
        })?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use crate::secrets::MemoryKeystore;

    use super::*;

    #[test]
    fn history_key_is_32_bytes() {
        let ks = MemoryKeystore::default();
        let key = history_key(&ks).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn history_key_is_stable_across_calls_on_the_same_keystore() {
        let ks = MemoryKeystore::default();
        let first = history_key(&ks).unwrap();
        let second = history_key(&ks).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn history_key_differs_across_independent_keystores() {
        // Astronomically unlikely to collide for two random 256-bit keys;
        // this guards against a hardcoded/all-zero key regression, not
        // against a genuine (near-impossible) collision.
        let a = history_key(&MemoryKeystore::default()).unwrap();
        let b = history_key(&MemoryKeystore::default()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn history_key_round_trips_through_the_keystore_as_hex() {
        let ks = MemoryKeystore::default();
        let key = history_key(&ks).unwrap();

        let stored = ks.get(SecretName::HISTORY_KEY).unwrap().unwrap();
        assert_eq!(stored.len(), 64);
        assert!(stored.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(decode_hex(&stored).unwrap(), key);
    }

    #[test]
    fn history_key_refuses_a_non_persistent_keystore() {
        let ks = MemoryKeystore::non_persistent();
        assert!(history_key(&ks).is_err());
        assert_eq!(ks.get(SecretName::HISTORY_KEY).unwrap(), None);
    }

    #[test]
    fn decode_hex_rejects_the_wrong_length() {
        assert!(decode_hex("abcd").is_err());
    }

    #[test]
    fn decode_hex_rejects_non_hex_characters() {
        let not_hex = "z".repeat(64);
        assert!(decode_hex(&not_hex).is_err());
    }

    #[test]
    fn encode_then_decode_hex_round_trips() {
        let bytes = [7u8; 32];
        assert_eq!(decode_hex(&encode_hex(&bytes)).unwrap(), bytes);
    }
}
