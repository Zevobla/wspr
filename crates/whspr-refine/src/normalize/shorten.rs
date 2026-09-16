//! J-11: honest, rule-based transcript shortening -- the pass wired up by
//! `NormalizingRefiner::with_shorten` (see its doc comment) and driven end
//! to end by whspr-cli's `[capture].shorten` / `--shorten` toggle. Three
//! sub-passes, applied in order:
//!   1. drop filler tokens: "um", "uh", "erm", "hmm", "you know", "I mean",
//!      "sort of", "kind of" are always dropped; "like" is dropped only
//!      when it's immediately followed by a comma or starts a clause (the
//!      very first word, or the word right after `. , ; : ! ?`) -- so "I
//!      like turtles" keeps its "like" (a genuine verb) while "like, I
//!      think" and "well, like, it works" both lose theirs.
//!   2. collapse immediate word repetitions ("the the" -> "the", "I I
//!      think" -> "I think"), reusing the same conservative machinery as
//!      the crate's F-19 `dedup` pass.
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

/// Filler phrases dropped as a unit, unconditionally.
const PHRASE_FILLERS: &[&[&str]] = &[
    &["you", "know"],
    &["i", "mean"],
    &["sort", "of"],
    &["kind", "of"],
];

/// Single-word fillers dropped unconditionally when they appear as a
/// standalone word. "like" is handled separately below since it's only
/// sometimes a filler.
const WORD_FILLERS: &[&str] = &["um", "uh", "erm", "hmm"];

fn core_lower(word: &str) -> String {
    split_punct(word).0.to_lowercase()
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

/// Drops filler tokens per the rules in the module doc comment above.
fn strip_shorten_fillers(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(words.len());

    let mut i = 0;
    while i < words.len() {
        let matched_phrase = i + 1 < words.len()
            && PHRASE_FILLERS
                .iter()
                .any(|p| core_lower(words[i]) == p[0] && core_lower(words[i + 1]) == p[1]);
        if matched_phrase {
            i += 2;
            continue;
        }

        let c = core_lower(words[i]);
        if !c.is_empty() && WORD_FILLERS.contains(&c.as_str()) {
            i += 1;
            continue;
        }

        if c == "like" {
            let (_, _, suffix) = split_punct(words[i]);
            let followed_by_comma = suffix.starts_with(',');
            if followed_by_comma || is_clause_start(kept.last().copied()) {
                i += 1;
                continue;
            }
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
    fn drops_unconditional_word_fillers() {
        let cases = [
            ("um so we should meet", "so we should meet"),
            ("uh let's go", "let's go"),
            ("it was erm difficult", "it was difficult"),
            ("hmm interesting", "interesting"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn drops_unconditional_phrase_fillers() {
        let cases = [
            ("you know, it works", "it works"),
            ("I mean, it's fine", "it's fine"),
            ("it's sort of working", "it's working"),
            ("it's kind of tired", "it's tired"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn drops_like_only_before_a_comma_or_at_clause_start() {
        assert_eq!(shorten("Like I said, thanks"), "I said, thanks");
        assert_eq!(shorten("I think, like, it works"), "I think, it works");
        assert_eq!(shorten("well, like it works"), "well, it works");
    }

    #[test]
    fn keeps_like_as_a_verb_mid_clause() {
        assert_eq!(shorten("I like turtles"), "I like turtles");
        assert_eq!(
            shorten("we really like this plan"),
            "we really like this plan"
        );
    }

    #[test]
    fn collapses_immediate_repetitions() {
        assert_eq!(shorten("the the cat sat"), "the cat sat");
        assert_eq!(shorten("I I think so"), "I think so");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(shorten("hello    world"), "hello world");
        assert_eq!(shorten("  leading and trailing  "), "leading and trailing");
    }

    #[test]
    fn preserves_punctuation_and_capitalization_of_kept_text() {
        assert_eq!(
            shorten("Um, Please call John, I mean Jane."),
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
        let input = "well um, like, the the thing is sort of kind of broken, you know";
        let once = shorten(input);
        let twice = shorten(&once);
        assert_eq!(once, twice);
    }
}
