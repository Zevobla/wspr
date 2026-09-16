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

use crate::hex::encode_hex;

/// Marks an encrypted history line (format version 1).
pub const ENCRYPTED_PREFIX: &str = "enc1:";

/// Nonce length for ChaCha20-Poly1305.
const NONCE_LEN: usize = 12;

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
