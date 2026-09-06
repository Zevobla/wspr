//! Sanitising whisper.cpp transcript text: stripping the model/service
//! tokens that can otherwise leak into the returned transcript (AM-19),
//! while leaving ordinary bracketed speech untouched.

/// Whether the text inside a `[...]` pair is a whisper.cpp service token
/// rather than ordinary bracketed speech.
///
/// Recognises two shapes: control tokens, whose inner text starts with `_`
/// (e.g. `[_BEG_]`, `[_TT_123]`, `[_EOT_]`), and all-caps service tags
/// (e.g. `[BLANK_AUDIO]`, `[MUSIC]`, `[NO SPEECH]`). Ordinary bracketed
/// words a person might actually say — `[note]`, `[важно]`, `[Name]` — have
/// lowercase or non-ASCII letters and are deliberately left alone.
fn is_model_bracket_token(inner: &str) -> bool {
    if inner.starts_with('_') {
        return true;
    }
    // An all-caps tag: at least two chars, at least one A-Z, and nothing
    // but uppercase letters, digits, spaces, and underscores.
    let mut has_upper = false;
    for c in inner.chars() {
        if c.is_ascii_uppercase() {
            has_upper = true;
        } else if !(c.is_ascii_digit() || c == ' ' || c == '_') {
            return false;
        }
    }
    has_upper && inner.len() >= 2
}

/// Removes whisper.cpp model/service tokens that can leak into segment text
/// (AM-19): `<|...|>` tokens (special tokens and `<|0.00|>`-style
/// timestamps), `[_..._]` control tokens, and all-caps `[...]` service tags
/// like `[BLANK_AUDIO]`/`[MUSIC]`. Ordinary bracketed words in real speech
/// are preserved (see [`is_model_bracket_token`]). Whitespace left behind by
/// a removed token is collapsed and the result is trimmed — only whitespace
/// is dropped, never a content character (AM-04).
pub(crate) fn strip_special_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<|") {
            // Drop the whole `<|...|>` token, including its delimiters.
            if let Some(close) = after.find("|>") {
                rest = &after[close + 2..];
                continue;
            }
            // No closing delimiter: keep the stray `<|` verbatim.
            out.push_str("<|");
            rest = after;
            continue;
        }

        if rest.starts_with('[') {
            if let Some(close) = rest[1..].find(']') {
                if is_model_bracket_token(&rest[1..1 + close]) {
                    rest = &rest[1 + close + 1..];
                    continue;
                }
            }
            // Not a model token (or unterminated): keep the `[` verbatim.
            out.push('[');
            rest = &rest[1..];
            continue;
        }

        let ch = rest.chars().next().expect("rest is non-empty");
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }

    // Collapse any whitespace the removals left behind, and trim the ends.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_special_tokens_removes_angle_bracket_tokens() {
        // Special tokens and `<|0.00|>`-style timestamps both use `<|...|>`.
        assert_eq!(
            strip_special_tokens(
                "<|startoftranscript|><|en|><|transcribe|> hello world<|endoftext|>"
            ),
            "hello world"
        );
        assert_eq!(
            strip_special_tokens("<|0.00|> one two three <|5.00|>"),
            "one two three"
        );
    }

    #[test]
    fn strip_special_tokens_removes_underscore_control_tokens() {
        assert_eq!(
            strip_special_tokens("[_BEG_] hello [_TT_123] world"),
            "hello world"
        );
        assert_eq!(strip_special_tokens("[_EOT_]"), "");
    }

    #[test]
    fn strip_special_tokens_removes_all_caps_service_tags() {
        assert_eq!(strip_special_tokens("[BLANK_AUDIO]"), "");
        assert_eq!(strip_special_tokens("hello [MUSIC] world"), "hello world");
        assert_eq!(strip_special_tokens("[NO SPEECH]"), "");
        // The removal must not leave a doubled space behind.
        assert_eq!(strip_special_tokens("a [MUSIC] b"), "a b");
    }

    #[test]
    fn strip_special_tokens_preserves_ordinary_bracketed_words() {
        // Lowercase / non-ASCII / mixed-case bracketed words are real speech,
        // not model tokens, and must survive verbatim.
        assert_eq!(strip_special_tokens("[важно] текст"), "[важно] текст");
        assert_eq!(
            strip_special_tokens("note [aside] here"),
            "note [aside] here"
        );
        assert_eq!(strip_special_tokens("plan [Name]"), "plan [Name]");
    }

    #[test]
    fn strip_special_tokens_leaves_plain_and_unterminated_text_alone() {
        assert_eq!(strip_special_tokens("just normal text"), "just normal text");
        // Unterminated tokens are kept verbatim rather than eating the rest
        // of the string.
        assert_eq!(strip_special_tokens("unclosed [MUSIC"), "unclosed [MUSIC");
        assert_eq!(strip_special_tokens("dangling <|foo"), "dangling <|foo");
    }

    #[test]
    fn strip_special_tokens_is_conservative_about_short_brackets() {
        // A single letter or a digits-only bracket isn't a known service
        // tag, so it's left alone.
        assert_eq!(strip_special_tokens("option [A] here"), "option [A] here");
        assert_eq!(strip_special_tokens("clause [123]"), "clause [123]");
    }

    /// AM-04: stripping a leading token must not swallow the first character
    /// of the actual utterance, whether or not a space separates them.
    #[test]
    fn strip_special_tokens_keeps_the_leading_utterance_character() {
        assert_eq!(
            strip_special_tokens("[_BEG_]One two three"),
            "One two three"
        );
        assert_eq!(strip_special_tokens("<|0.00|>One two"), "One two");
        assert_eq!(strip_special_tokens("[_BEG_] Once upon"), "Once upon");
        // A realistic whisper segment with a mix of leading tokens.
        assert_eq!(
            strip_special_tokens("<|0.00|> [_BEG_] Hello [MUSIC] world [BLANK_AUDIO]"),
            "Hello world"
        );
    }
}
