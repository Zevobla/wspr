//! AJ-01/AJ-02: Emacs-abbrev-style macro expansion.
//!
//! A user-defined trigger phrase ("my email") is replaced with its
//! configured expansion ("me@example.com") wherever it appears in the
//! dictated text. Triggers match case-insensitively on whole words --
//! "myemail" never counts as containing the trigger "email". Longer
//! triggers (by word count) are tried before shorter ones at each
//! position, so a trigger that is a prefix of another trigger's words
//! (e.g. "call" vs "call mom") never shadows the longer, more specific one.

use std::collections::BTreeMap;

use super::split_punct;

/// Expands every occurrence of a macro trigger phrase in `text` into its
/// configured expansion. `macros` is empty by default, in which case this
/// is a no-op and `text` is returned unchanged.
pub fn expand_macros(text: &str, macros: &BTreeMap<String, String>) -> String {
    if macros.is_empty() {
        return text.to_string();
    }

    // Each trigger's words, lowercased for case-insensitive comparison,
    // paired with its expansion. Sorted longest-first (by word count) so a
    // multi-word trigger wins over a shorter one that only matches its
    // first word(s); ties broken by the words themselves for determinism.
    let mut triggers: Vec<(Vec<String>, &str)> = macros
        .iter()
        .map(|(phrase, expansion)| {
            let words: Vec<String> = phrase.split_whitespace().map(str::to_lowercase).collect();
            (words, expansion.as_str())
        })
        .filter(|(words, _)| !words.is_empty())
        .collect();
    triggers.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<String> = words
        .iter()
        .map(|w| split_punct(w).0.to_lowercase())
        .collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let hit = triggers.iter().find_map(|(trigger, expansion)| {
            let end = i + trigger.len();
            (end <= cores.len() && cores[i..end] == trigger[..]).then_some((end, *expansion))
        });

        if let Some((end, expansion)) = hit {
            let (_, prefix, _) = split_punct(words[i]);
            let (_, _, suffix) = split_punct(words[end - 1]);
            out.push(format!("{prefix}{expansion}{suffix}"));
            i = end;
        } else {
            out.push(words[i].to_string());
            i += 1;
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macros(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn expands_a_multi_word_trigger() {
        let m = macros(&[("my email", "me@example.com")]);
        assert_eq!(
            expand_macros("send my email please", &m),
            "send me@example.com please"
        );
    }

    #[test]
    fn matches_case_insensitively() {
        let m = macros(&[("my email", "me@example.com")]);
        assert_eq!(
            expand_macros("Send MY EMAIL please", &m),
            "Send me@example.com please"
        );
    }

    #[test]
    fn does_not_expand_inside_a_larger_word() {
        let m = macros(&[("email", "EMAIL")]);
        assert_eq!(
            expand_macros("myemail is not real", &m),
            "myemail is not real"
        );
        let m = macros(&[("my email", "me@example.com")]);
        assert_eq!(expand_macros("my emailing list", &m), "my emailing list");
    }

    #[test]
    fn expands_multiple_distinct_macros() {
        let m = macros(&[("my email", "me@example.com"), ("my phone", "555-1234")]);
        assert_eq!(
            expand_macros("call my phone or send my email", &m),
            "call 555-1234 or send me@example.com"
        );
    }

    #[test]
    fn empty_map_is_a_noop() {
        let input = "send my email please";
        assert_eq!(expand_macros(input, &BTreeMap::new()), input);
    }

    #[test]
    fn longest_trigger_wins_over_a_shorter_prefix() {
        let m = macros(&[("call", "C"), ("call mom", "CALL_MOM")]);
        assert_eq!(
            expand_macros("please call mom now", &m),
            "please CALL_MOM now"
        );
    }

    #[test]
    fn preserves_surrounding_punctuation() {
        let m = macros(&[("my email", "me@example.com")]);
        assert_eq!(
            expand_macros("my email, thanks", &m),
            "me@example.com, thanks"
        );
        assert_eq!(expand_macros("(my email)", &m), "(me@example.com)");
    }
}
