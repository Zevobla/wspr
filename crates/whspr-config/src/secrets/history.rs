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
pub fn history_key(ks: &dyn Keystore) -> whspr_core::Result<[u8; 32]> {
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
