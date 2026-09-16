//! J-11: honest, rule-based transcript shortening -- the pass wired up by
//! `NormalizingRefiner::with_shorten` (see its doc comment) and driven end
//! to end by whspr-cli's `[capture].shorten` / `--shorten` toggle.
//!
//! This is where every AMBIGUOUS filler lives -- every word/phrase that is
//! also real content in some other sentence, and therefore isn't safe for
//! the crate's always-on `fillers` pass (see that module's doc for the
//! corruption bugs that caused this split). Three sub-passes, applied in
//! order:
//!   1. drop filler tokens, in three tiers by how they're disambiguated:
//!      - unconditional: "в общем", "так сказать", "это самое" carry no
//!        content in any reading, so no position check is needed
//!      - parenthetical-gated: "like", "you know", "i mean", "как бы",
//!        and Russian "ну"/"нуу"/"короче"/"типа"/"значит"/"вот"/"блин" are
//!        all also real words/phrases ("короче" = "shorter", "блин" =
//!        "pancake", ...), so each is dropped only when set off as a
//!        parenthetical aside: at clause start (text start, or right
//!        after `. ! ? ,`) or immediately followed by a comma -- "Like I
//!        said, thanks" and "well, like, it works" lose theirs; "I like
//!        turtles" (a verb, mid-clause) keeps it
//!      - preceded-word-gated: "sort of"/"kind of" are a hedge ("it's
//!        sort of ready" -> "it's ready") UNLESS the word right before
//!        them is a determiner or wh-word ("a", "what", "the", ...), in
//!        which case they're the real qualifier in "what kind of car" /
//!        "a sort of fish" and must survive
//!   2. collapse immediate word repetitions ("the the" -> "the", "I I
//!      think" -> "I think"), reusing the same conservative machinery as
//!      the crate's F-19 `dedup` pass -- which itself never touches a
//!      repeated digit string or number word (F-10's table), so a
//!      genuinely dictated "five five five one two" survives intact
//!   3. collapse whitespace left behind by the first two passes.
//!
//! Every kept word's own punctuation and capitalization are left exactly
//! as they were.
//!
//! Called from `apply` right after macro/dictionary expansion (so a
//! trigger phrase containing one of the words above -- e.g. a macro
//! literally triggered by "kind of urgent" -- still matches the raw
//! dictation, not text this pass has already eaten into) and well before
//! `paragraph_break` (which must run dead last: this pass tokenizes on
//! whitespace the same way every other pass here does, which would
//! swallow an inserted blank-line break). See `apply`'s call site for the
//! exact position and its own ordering tests.

use super::dedup::collapse_duplicate_words;
use super::split_punct;

/// Filler phrases dropped as a unit, unconditionally -- unlike the
/// parenthetical-gated ones below, none of these carries content in any
/// reading, so no sentence-position check is needed.
const UNCONDITIONAL_PHRASE_FILLERS: &[&[&str]] =
    &[&["в", "общем"], &["так", "сказать"], &["это", "самое"]];

/// Filler phrases dropped only when set off as a parenthetical (clause
/// start, or immediately followed by a comma) -- each can also be read as
/// literal content ("you know" as a real question, "как бы" as "as if"),
/// so position is what disambiguates a filler use from a real one.
const PARENTHETICAL_PHRASE_FILLERS: &[&[&str]] =
    &[&["you", "know"], &["i", "mean"], &["как", "бы"]];

/// Single-word fillers dropped only when set off as a parenthetical, for
/// the same reason as the phrases above -- every one of these is also a
/// real word: "ну" doubles as an interjection, "короче" as "shorter",
/// "типа" as "like [a type of]", "значит" as "means/so", "вот" as "here
/// [is]", "блин" as "pancake". "like" is the English member of this set.
const PARENTHETICAL_WORD_FILLERS: &[&str] = &[
    "like",
    "ну",
    "нуу",
    "короче",
    "типа",
    "значит",
    "вот",
    "блин",
];

/// "sort of"/"kind of" as a phrase pair, handled by their own
/// preceded-word rule below rather than the parenthetical one.
const KIND_SORT_PHRASES: &[&[&str]] = &[&["sort", "of"], &["kind", "of"]];

/// Determiners/wh-words that make a following "sort of"/"kind of" the
/// real qualifier ("what kind of car", "a sort of fish") rather than a
/// hedge, so it must survive.
const DETERMINERS: &[&str] = &[
    "a",
    "an",
    "the",
    "this",
    "that",
    "these",
    "those",
    "what",
    "which",
    "some",
    "any",
    "every",
    "each",
    "no",
    "one",
    "same",
    "other",
    "another",
    "different",
];

fn core_lower(word: &str) -> String {
    split_punct(word).0.to_lowercase()
}

/// Whether `words[i..]` starts with one of the two-word `phrases`,
/// case-insensitively on each word's core.
fn matches_two_word_phrase(words: &[&str], i: usize, phrases: &[&[&str]]) -> bool {
    i + 1 < words.len()
        && phrases
            .iter()
            .any(|p| core_lower(words[i]) == p[0] && core_lower(words[i + 1]) == p[1])
}

/// Whether the word right after `prev_kept` (the previous *kept* token, or
/// `None` for the very first word) starts a new clause: either there is no
/// previous word, or the previous word ends in clause/sentence punctuation.
fn is_clause_start(prev_kept: Option<&str>) -> bool {
    match prev_kept {
        None => true,
        Some(prev) => {
            let (_, _, suffix) = split_punct(prev);
            suffix.contains(['.', ',', ';', ':', '!', '?'])
        }
    }
}

/// Whether the phrase/word ending at `last_word` (its own trailing
/// punctuation) or starting right after `prev_kept` counts as set off as a
/// parenthetical -- the shared gate for every parenthetical-tier filler.
fn is_parenthetical(prev_kept: Option<&str>, last_word: &str) -> bool {
    let (_, _, suffix) = split_punct(last_word);
    suffix.starts_with(',') || is_clause_start(prev_kept)
}

/// Drops filler tokens per the rules in the module doc comment above.
fn strip_shorten_fillers(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(words.len());

    let mut i = 0;
    while i < words.len() {
        if matches_two_word_phrase(&words, i, UNCONDITIONAL_PHRASE_FILLERS) {
            i += 2;
            continue;
        }

        if matches_two_word_phrase(&words, i, KIND_SORT_PHRASES) {
            let preceded_by_determiner = kept
                .last()
                .is_some_and(|prev| DETERMINERS.contains(&core_lower(prev).as_str()));
            if !preceded_by_determiner {
                i += 2;
                continue;
            }
            // Real qualifier, not a hedge -- keep both words untouched by
            // falling through to the ordinary push below for this one,
            // then let the next iteration handle "of" normally.
        } else if matches_two_word_phrase(&words, i, PARENTHETICAL_PHRASE_FILLERS)
            && is_parenthetical(kept.last().copied(), words[i + 1])
        {
            i += 2;
            continue;
        }

        let c = core_lower(words[i]);
        if PARENTHETICAL_WORD_FILLERS.contains(&c.as_str())
            && is_parenthetical(kept.last().copied(), words[i])
        {
            i += 1;
            continue;
        }

        kept.push(words[i]);
        i += 1;
    }

    kept.join(" ")
}

/// Tidies spacing left behind by token removal: no space before common
/// punctuation, and any run of whitespace collapsed to a single space.
fn tidy_spacing(text: &str) -> String {
    text.replace(" ,", ",")
        .replace(" .", ".")
        .replace(" !", "!")
        .replace(" ?", "?")
        .replace(" :", ":")
        .replace(" ;", ";")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Applies the full shorten pass: filler removal, immediate-repetition
/// collapse, then a whitespace tidy. Idempotent -- running it twice never
/// differs from running it once, since nothing it does creates a new
/// filler token, a new repeated word, or new stray whitespace (see the
/// `is_idempotent` test below).
pub fn shorten(text: &str) -> String {
    let text = tidy_spacing(&strip_shorten_fillers(text));
    tidy_spacing(&collapse_duplicate_words(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_unconditional_phrase_fillers() {
        let cases = [
            ("всё в общем понятно", "всё понятно"),
            ("он так сказать ушёл", "он ушёл"),
            ("это самое непонятно", "непонятно"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn drops_parenthetical_word_fillers_only_when_set_off() {
        let cases = [
            ("ну, короче, это работает", "это работает"),
            ("Like I said, thanks", "I said, thanks"),
            ("I think, like, it works", "I think, it works"),
            ("well, like it works", "well, it works"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn keeps_parenthetical_word_fillers_mid_clause() {
        let cases = [
            ("I like turtles", "I like turtles"),
            ("we really like this plan", "we really like this plan"),
            (
                "сделай его короче пожалуйста",
                "сделай его короче пожалуйста",
            ),
            ("данные значит важны", "данные значит важны"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn drops_parenthetical_phrase_fillers_only_when_set_off() {
        let cases = [
            ("you know, it works", "it works"),
            ("I mean, it's fine", "it's fine"),
            ("well, как бы, it's fine", "well, it's fine"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn keeps_parenthetical_phrase_fillers_mid_clause() {
        assert_eq!(shorten("what I mean is this"), "what I mean is this");
    }

    #[test]
    fn kind_sort_of_dropped_as_a_hedge() {
        let cases = [
            ("it's sort of ready", "it's ready"),
            ("it's kind of tired", "it's tired"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn kind_sort_of_kept_after_a_determiner_or_wh_word() {
        let cases = [
            ("what kind of car", "what kind of car"),
            ("a sort of fish", "a sort of fish"),
            ("which kind of test", "which kind of test"),
            ("some sort of plan", "some sort of plan"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn collapses_immediate_repetitions() {
        assert_eq!(shorten("the the cat sat"), "the cat sat");
        assert_eq!(shorten("I I think so"), "I think so");
    }

    #[test]
    fn never_collapses_repeated_number_words() {
        assert_eq!(shorten("five five five one two"), "five five five one two");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(shorten("hello    world"), "hello world");
        assert_eq!(shorten("  leading and trailing  "), "leading and trailing");
    }

    #[test]
    fn preserves_punctuation_and_capitalization_of_kept_text() {
        assert_eq!(
            shorten("Please call John, I mean, Jane."),
            "Please call John, Jane."
        );
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        let input = "The quick brown fox jumps over the lazy dog.";
        assert_eq!(shorten(input), input);
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(shorten(""), "");
    }

    #[test]
    fn is_idempotent() {
        let input = "well ну, короче, the the thing is sort of kind of broken, you know";
        let once = shorten(input);
        let twice = shorten(&once);
        assert_eq!(once, twice);
    }
}
