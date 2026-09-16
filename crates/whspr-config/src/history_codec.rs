//! The per-line format of `history.jsonl` when `[privacy].history_encryption`
//! is on. A plaintext line is one JSON object, as it has always been; an
//! encrypted line is [`ENCRYPTED_PREFIX`] followed by lowercase hex of a
//! random 12-byte nonce and the ChaCha20-Poly1305 ciphertext of that JSON
//! (authentication tag included). Both kinds can sit in one file, so a
//! reader handles a file mid-way through being switched either way.
//!
//! The key is [`crate::history_key`]'s 32 bytes. This module never touches a
//! keystore or a file -- it only encodes and decodes lines.

use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::OsRng;
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit, Nonce};
use whspr_core::{Result, WhsprError};

use crate::hex::{decode_hex, encode_hex};

/// Marks an encrypted history line (format version 1).
pub const ENCRYPTED_PREFIX: &str = "enc1:";

/// Nonce length for ChaCha20-Poly1305.
const NONCE_LEN: usize = 12;
/// Poly1305 authentication tag length.
const TAG_LEN: usize = 16;

/// Encrypts one history line's `json` under `key`, with a fresh random nonce
/// from the OS, as `enc1:` + hex(nonce ‖ ciphertext ‖ tag).
pub fn encode_line(json: &str, key: &[u8; 32]) -> String {
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let mut sealed = json.as_bytes().to_vec();
    ChaCha20Poly1305::new(&(*key).into())
        .encrypt_in_place(&Nonce::from(nonce), b"", &mut sealed)
        .expect("ChaCha20-Poly1305 only rejects plaintexts larger than 256 GiB");
    let mut payload = nonce.to_vec();
    payload.extend_from_slice(&sealed);
    format!("{ENCRYPTED_PREFIX}{}", encode_hex(&payload))
}

/// Reads one history line back into its JSON text.
///
/// - A blank line is `Ok(None)`.
/// - A plaintext line passes through unchanged as `Ok(Some(line))`.
/// - An `enc1:` line is decrypted with `key`.
///
/// An encrypted line is an error when there is no key, its hex is malformed,
/// it is too short to hold a nonce and tag, it fails authentication (a wrong
/// key or tampering) or it does not decrypt to UTF-8. Callers skip such a
/// line and count it; nothing here panics.
pub fn decode_line(line: &str, key: Option<&[u8; 32]>) -> Result<Option<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let Some(payload) = trimmed.strip_prefix(ENCRYPTED_PREFIX) else {
        return Ok(Some(line.to_string()));
    };
    let key = key.ok_or_else(|| codec_error("it is encrypted and no history key is available"))?;
    let bytes = decode_hex(payload).ok_or_else(|| codec_error("its payload is not hex"))?;
    if bytes.len() < NONCE_LEN + TAG_LEN {
        return Err(codec_error("it is too short to be encrypted history"));
    }
    let (nonce, sealed) = bytes.split_at(NONCE_LEN);
    let mut plain = sealed.to_vec();
    ChaCha20Poly1305::new(&(*key).into())
        .decrypt_in_place(Nonce::from_slice(nonce), b"", &mut plain)
        .map_err(|_| codec_error("it failed authentication (wrong key or tampered)"))?;
    String::from_utf8(plain)
        .map(Some)
        .map_err(|_| codec_error("it did not decrypt to UTF-8 text"))
}

/// A history-line error explaining `why` the line could not be read.
fn codec_error(why: &str) -> WhsprError {
    WhsprError::Other(format!("unreadable history line: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];
    const JSON: &str = r#"{"text":"hello there","duration_secs":1.5}"#;

    #[test]
    fn an_encrypted_line_round_trips() {
        let line = encode_line(JSON, &KEY);
        assert!(line.starts_with(ENCRYPTED_PREFIX));
        assert!(!line.contains("hello"));
        assert_eq!(
            decode_line(&line, Some(&KEY)).unwrap().as_deref(),
            Some(JSON)
        );
    }

    #[test]
    fn a_tampered_line_fails_authentication() {
        let line = encode_line(JSON, &KEY);
        // Flip one ciphertext digit, well past the prefix and nonce.
        let mut tampered: Vec<char> = line.chars().collect();
        let at = ENCRYPTED_PREFIX.len() + 2 * NONCE_LEN + 4;
        tampered[at] = if tampered[at] == '0' { '1' } else { '0' };
        let tampered: String = tampered.into_iter().collect();

        assert!(decode_line(&tampered, Some(&KEY)).is_err());
    }

    #[test]
    fn a_line_under_another_key_is_rejected() {
        let line = encode_line(JSON, &KEY);
        assert!(decode_line(&line, Some(&[8; 32])).is_err());
    }

    #[test]
    fn an_encrypted_line_without_a_key_is_an_error() {
        let line = encode_line(JSON, &KEY);
        assert!(decode_line(&line, None).is_err());
    }

    #[test]
    fn plaintext_lines_pass_through_with_or_without_a_key() {
        assert_eq!(decode_line(JSON, None).unwrap().as_deref(), Some(JSON));
        assert_eq!(
            decode_line(JSON, Some(&KEY)).unwrap().as_deref(),
            Some(JSON)
        );
        assert_eq!(decode_line("   ", Some(&KEY)).unwrap(), None);
    }
}
