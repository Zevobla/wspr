//! G-16: spoken punctuation words rewritten as marks.
//!
//! "hello comma world" -> "hello, world", "wait period stop" -> "wait.
//! stop", and the Russian equivalents "запятая"/"точка". The command word
//! is dropped and its mark is glued onto the *previous* kept word (no
//! space before a comma/period, same convention every other pass in this
//! module already reproduces for ordinary punctuation).
//!
//! This is the classic "say 'comma'/'period' to insert punctuation"
//! dictation convention (Dragon NaturallySpeaking and friends), applied
//! unconditionally to every occurrence of these four words - which is
//! exactly why it's a toggle: a user who actually talks about a "grace
//! period" or "shopping list, comma-separated" wants it off. Deliberately
//! excludes "point" - that word is reserved for the decimal-number
//! grammar in `numbers::decimals` ("three point five" -> "3.5").

use super::split_punct;

/// The punctuation mark a spoken command word stands for, or `None` if
/// `core` isn't one of the four recognized words.
fn punctuation_word(core: &str) -> Option<char> {
    match core.to_lowercase().as_str() {
        "comma" | "запятая" => Some(','),
        "period" | "точка" => Some('.'),
        _ => None,
    }
}

/// Replaces every spoken punctuation command word with its mark, glued
/// onto the previous word (or emitted bare, if it's the very first word).
pub fn normalize_punctuation_words(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(' ') {
        let (core, _, suffix) = split_punct(word);
        if let Some(mark) = punctuation_word(core) {
            match out.last_mut() {
                Some(last) => {
                    last.push(mark);
                    last.push_str(suffix);
                }
                None => out.push(format!("{mark}{suffix}")),
            }
        } else {
            out.push(word.to_string());
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_comma_and_period() {
        assert_eq!(
            normalize_punctuation_words("hello comma world"),
            "hello, world"
        );
        assert_eq!(
            normalize_punctuation_words("wait period stop"),
            "wait. stop"
        );
    }

    #[test]
    fn russian_zapyataya_and_tochka() {
        assert_eq!(
            normalize_punctuation_words("привет запятая как дела точка"),
            "привет, как дела."
        );
    }

    #[test]
    fn multiple_marks_in_one_sentence() {
        assert_eq!(
            normalize_punctuation_words("first comma second comma third period"),
            "first, second, third."
        );
    }

    #[test]
    fn mark_at_the_very_start_is_emitted_bare() {
        assert_eq!(normalize_punctuation_words("comma world"), ", world");
    }

    #[test]
    fn does_not_touch_point_reserved_for_decimals() {
        assert_eq!(
            normalize_punctuation_words("three point five"),
            "three point five"
        );
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(normalize_punctuation_words("hello world"), "hello world");
        assert_eq!(normalize_punctuation_words(""), "");
    }
}
