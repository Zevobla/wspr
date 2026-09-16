//! Lowercase hex encoding shared by the history-encryption key (a keystore
//! entry is text) and the encrypted history line format
//! (`history_codec`).

/// Lowercase-hex-encodes `bytes` (two characters per byte).
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

/// Decodes hex digits (either case) back into bytes. `None` for an odd
/// number of digits or any non-hex character -- including a sign or a
/// multi-byte UTF-8 character, which are rejected rather than parsed or
/// sliced through.
pub(crate) fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let (pairs, leftover) = hex.as_bytes().as_chunks::<2>();
    if !leftover.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|&[high, low]| Some((nibble(high)? << 4) | nibble(low)?))
        .collect()
}

/// The value of one ASCII hex digit.
fn nibble(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}
