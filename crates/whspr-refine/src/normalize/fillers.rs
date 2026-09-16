//! Removes UNAMBIGUOUS speech-filler hesitation sounds -- ones with no
//! other meaning in the language -- that pad spoken input between real
//! words. Rule-based, so it runs even with the noop refiner, for every
//! dictation (unlike the LLM prompt's own filler removal, and unlike the
//! opt-in `shorten` pass, which owns everything ambiguous).
//!
//! Deliberately narrow, because this pass is always on and unconditional:
//! every word here is context-free safe to drop -- it is never also a
//! real word, a unit, or a conjunction. In particular this pass does NOT
//! touch:
//!   - "like"/"you know"/"i mean" -- real content in "I like turtles" /
//!     "what I mean is"
//!   - Russian "ну"/"нуу"/"короче"/"типа"/"значит"/"вот"/"блин" -- all
//!     real words ("короче" = "shorter", "типа" = "like [a type of]",
//!     "значит" = "means/so", "вот" = "here [is]", "блин" = "pancake")
//!   - "как бы"/"это самое"/"в общем"/"так сказать" -- real phrases
//!   - single/double Russian letters that double as units or a
//!     conjunction: "а" (also "but"/"and"), "м"/"мм" (also meter/
//!     millimeter abbreviations), "мс" (also a millisecond abbreviation)
//! Every one of those moved to the opt-in `shorten` pass (see its module
//! doc), which drops the ambiguous ones only when sentence position marks
//! them as a parenthetical aside rather than real content -- exactly the
//! distinction this always-on pass can't afford to guess at, since it
//! runs on every dictation whether the user wants shortening or not.
//!
//! Conservative even within what it does drop: only whole-word,
//! case-insensitive matches, and the leftover spacing/punctuation is
//! tidied up so removing a filler never leaves a double space or a space
//! before a comma.

/// Single-word fillers dropped when they appear as a standalone word
/// (case-insensitive, surrounding punctuation ignored). English
/// hesitation sounds, plus Russian "эм" -- see the module doc for why nothing
/// else Russian is a fixed literal here (the rest are matched structurally
/// below, since "any length of э" and "3+ repeated м/а" aren't a finite list).
const WORD_FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "hmm", "эм"];

/// Lowercased word with any leading/trailing punctuation stripped, for
/// matching against the filler lists.
fn core(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// Whether `core` is a Russian hesitation sound made entirely of repeated
/// "э" (any length: "э", "ээ", "ээээ", ...). Unlike "а"/"м"/"мм", "э" on
/// its own has no other meaning in Russian, so any run of it is safe to
/// drop unconditionally.
fn is_e_hesitation(core: &str) -> bool {
    !core.is_empty() && core.chars().all(|c| c == 'э')
}

/// Whether `core` is an elongated "ммм.../ааа..." hesitation drawl: the
/// same letter repeated 3 or more times. Below 3 repeats, "аа" isn't a
/// distinct hesitation sound and "мм" is the millimeter abbreviation (see
/// the module doc), so the floor matters; at 3+, a drawn-out "mmmm"/
/// "aaaa" hesitation has no other reading.
fn is_elongated_hesitation(core: &str) -> bool {
    let mut chars = core.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == 'м' || first == 'а') && core.chars().all(|c| c == first) && core.chars().count() >= 3
}

/// Drops filler words from `text`, tidying the spacing left behind.
pub fn strip_fillers(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(words.len());

    for word in words {
        let c = core(word);
        let is_filler = !c.is_empty()
            && (WORD_FILLERS.contains(&c.as_str())
                || is_e_hesitation(&c)
                || is_elongated_hesitation(&c));
        if !is_filler {
            kept.push(word);
        }
    }

    // Rejoin and tidy: no space before common punctuation, collapse any runs.
    let joined = kept.join(" ");
    joined
        .replace(" ,", ",")
        .replace(" .", ".")
        .replace(" !", "!")
        .replace(" ?", "?")
        .replace(" :", ":")
        .replace(" ;", ";")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_e_and_elongated_hesitations() {
        assert_eq!(strip_fillers("эээ привет"), "привет");
        assert_eq!(strip_fillers("ммм да"), "да");
        assert_eq!(strip_fillers("ааа подожди"), "подожди");
        assert_eq!(strip_fillers("э это тест"), "это тест");
    }

    #[test]
    fn leaves_ambiguous_russian_words_alone() {
        // "ну"/"короче"/"типа"/"значит"/"вот"/"блин" are all real words
        // outside a filler reading -- moved to the opt-in `shorten` pass,
        // which can use sentence position to disambiguate. This
        // always-on pass must never touch them.
        assert_eq!(
            strip_fillers("ну короче я пошёл домой"),
            "ну короче я пошёл домой"
        );
        assert_eq!(
            strip_fillers("мм значит это работает"),
            "мм значит это работает"
        );
        assert_eq!(strip_fillers("сделай короче"), "сделай короче");
        assert_eq!(strip_fillers("данные типа int"), "данные типа int");
        assert_eq!(strip_fillers("испечь блин"), "испечь блин");
        assert_eq!(strip_fillers("вот дом"), "вот дом");
        assert_eq!(strip_fillers("это значит, что"), "это значит, что");
    }

    #[test]
    fn leaves_ambiguous_russian_phrases_alone() {
        assert_eq!(strip_fillers("это как бы важно"), "это как бы важно");
        assert_eq!(strip_fillers("как бы ты поступил"), "как бы ты поступил");
        assert_eq!(strip_fillers("ну в общем всё"), "ну в общем всё");
    }

    #[test]
    fn leaves_short_russian_tokens_that_double_as_units_or_a_conjunction() {
        // "а" is also the conjunction "but"/"and"; "м"/"мм" are also the
        // meter/millimeter abbreviations. None of these are hesitation
        // sounds on their own (only a 3+ run of the same letter is).
        assert_eq!(
            strip_fillers("я пошёл, а он остался"),
            "я пошёл, а он остался"
        );
        assert_eq!(strip_fillers("5 мм"), "5 мм");
        assert_eq!(strip_fillers("10 м"), "10 м");
        assert_eq!(strip_fillers("аа"), "аа");
        assert_eq!(strip_fillers("мм"), "мм");
    }

    #[test]
    fn removes_english_hesitation_sounds_only() {
        assert_eq!(strip_fillers("um so we should meet"), "so we should meet");
        assert_eq!(strip_fillers("uh let's go"), "let's go");
        // "like" is ambiguous (also a verb) -- left to `shorten`.
        assert_eq!(strip_fillers("I like it"), "I like it");
        assert_eq!(strip_fillers("what I mean is this"), "what I mean is this");
    }

    #[test]
    fn keeps_real_words_and_tidies_punctuation() {
        // A filler carrying an attached comma is dropped whole (the comma was
        // spurious padding around the filler anyway).
        assert_eq!(strip_fillers("привет эээ, как дела"), "привет как дела");
        // "ну" is no longer a filler here, but a stray space before a real
        // comma is still tidied.
        assert_eq!(strip_fillers("привет ну , как дела"), "привет ну, как дела");
        // Sentence with no fillers is unchanged (modulo whitespace collapse).
        assert_eq!(strip_fillers("обычное предложение"), "обычное предложение");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(strip_fillers(""), "");
    }
}
