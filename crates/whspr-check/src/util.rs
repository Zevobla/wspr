//! Small string helpers shared across check modules (mostly for trimming
//! subprocess output down to a readable amount of evidence text).

/// Last `n` bytes of `s`, snapped forward to the nearest UTF-8 char
/// boundary so this never panics on a multi-byte character straddling the
/// cut point.
pub fn tail(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut idx = s.len() - n;
    while !s.is_char_boundary(idx) {
        idx += 1;
    }
    &s[idx..]
}

/// First `n` bytes of `s`, snapped backward to the nearest UTF-8 char
/// boundary so this never panics on a multi-byte character straddling the
/// cut point.
pub fn head(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut idx = n;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_returns_whole_string_when_shorter_than_n() {
        assert_eq!(tail("hello", 10), "hello");
    }

    #[test]
    fn tail_truncates_to_last_n_bytes() {
        assert_eq!(tail("hello world", 5), "world");
    }

    #[test]
    fn tail_snaps_forward_off_a_multibyte_boundary() {
        // "héllo" - 'é' is 2 bytes, so byte offset 2 lands mid-character.
        let s = "héllo";
        let out = tail(s, s.len() - 2);
        assert!(s.ends_with(out));
        // Must not panic and must be valid UTF-8 (guaranteed by &str).
    }

    #[test]
    fn head_returns_whole_string_when_shorter_than_n() {
        assert_eq!(head("hello", 10), "hello");
    }

    #[test]
    fn head_truncates_to_first_n_bytes() {
        assert_eq!(head("hello world", 5), "hello");
    }

    #[test]
    fn head_snaps_backward_off_a_multibyte_boundary() {
        let s = "héllo";
        // byte 2 is mid-'é'; head should back off to byte 1 ("h").
        let out = head(s, 2);
        assert_eq!(out, "h");
    }
}
