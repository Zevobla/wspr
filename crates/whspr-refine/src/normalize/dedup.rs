//! F-19: consecutive duplicate words collapsed.
//!
//! "the the cat" -> "the cat", "и и потом" -> "и потом". A word is dropped
//! when its core equals the previous kept word's core (case-insensitively).
//! Guards keep this conservative:
//!   - only alphabetic words collapse, so a real repeat like "20 20" (a year
//!     said twice, two scores) is preserved;
//!   - the previous word must carry no trailing punctuation and the current
//!     word no leading punctuation, so a sentence boundary ("cat. Cat ran")
//!     or a bracketed aside is never merged away.

use super::split_punct;

fn is_collapsible(core: &str) -> bool {
    core.chars().any(char::is_alphabetic)
}

/// Collapses runs of the same word into a single occurrence, keeping the
/// first word's text and the last word's trailing punctuation.
pub fn collapse_duplicate_words(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(' ') {
        let (core, prefix, suffix) = split_punct(word);
        let is_dup = prefix.is_empty()
            && is_collapsible(core)
            && out.last().is_some_and(|prev| {
                let (prev_core, _, prev_suffix) = split_punct(prev);
                prev_suffix.is_empty() && prev_core.to_lowercase() == core.to_lowercase()
            });
        if is_dup {
            let prev = out.pop().expect("is_dup implies a previous word");
            let (prev_core, prev_prefix, _) = split_punct(&prev);
            out.push(format!("{prev_prefix}{prev_core}{suffix}"));
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
    fn collapses_simple_english_and_russian() {
        assert_eq!(collapse_duplicate_words("the the cat"), "the cat");
        assert_eq!(collapse_duplicate_words("и и потом"), "и потом");
    }

    #[test]
    fn collapses_case_insensitively_keeping_first() {
        assert_eq!(collapse_duplicate_words("The the cat"), "The cat");
    }

    #[test]
    fn collapses_three_in_a_row() {
        assert_eq!(collapse_duplicate_words("no no no way"), "no way");
    }

    #[test]
    fn keeps_last_words_trailing_punctuation() {
        assert_eq!(collapse_duplicate_words("bye bye."), "bye.");
        assert_eq!(collapse_duplicate_words("(the the)"), "(the)");
    }

    #[test]
    fn does_not_cross_sentence_or_clause_boundary() {
        // A period between the two words marks a boundary - not a stutter.
        assert_eq!(collapse_duplicate_words("cat. Cat ran"), "cat. Cat ran");
        // Purely numeric repeats are left alone (e.g. a year said twice).
        assert_eq!(collapse_duplicate_words("20 20"), "20 20");
    }

    #[test]
    fn leaves_distinct_words_untouched() {
        assert_eq!(collapse_duplicate_words("the cat sat"), "the cat sat");
        assert_eq!(collapse_duplicate_words("hello world"), "hello world");
        assert_eq!(collapse_duplicate_words(""), "");
    }
}
