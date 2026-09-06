//! Removes common speech-filler words / disfluencies that pad spoken input
//! between real words -- Russian "эээ", "ммм", "ну", "короче", "как бы",
//! "типа", "значит" and English "um", "uh", "like", "you know". Rule-based,
//! so it runs even with the noop refiner (unlike the LLM prompt's filler
//! removal). Conservative: only whole-word, case-insensitive matches are
//! dropped, and the leftover spacing / punctuation is tidied up so removing a
//! filler never leaves a double space or a space before a comma.

/// Single-word fillers dropped when they appear as a standalone word
/// (case-insensitive, surrounding punctuation ignored).
const WORD_FILLERS: &[&str] = &[
    // Russian hesitation sounds and verbal parasites
    "э",
    "ээ",
    "эээ",
    "ээээ",
    "а",
    "аа",
    "ааа",
    "м",
    "мм",
    "ммм",
    "мс",
    "эм",
    "ну",
    "нуу",
    "короче",
    "типа",
    "значит",
    "вот",
    "блин",
    // English
    "um",
    "umm",
    "uh",
    "uhh",
    "uhm",
    "erm",
    "hmm",
    "eh",
    "like",
];

/// Two-word filler phrases dropped as a unit (case-insensitive).
const PHRASE_FILLERS: &[&[&str]] = &[
    &["как", "бы"],
    &["это", "самое"],
    &["в", "общем"],
    &["так", "сказать"],
    &["you", "know"],
    &["i", "mean"],
];

/// Lowercased word with any leading/trailing punctuation stripped, for
/// matching against the filler lists. Returns the trimmed core and whether it
/// carried a trailing sentence punctuation mark we should preserve.
fn core(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// Drops filler words/phrases from `text`, tidying the spacing left behind.
pub fn strip_fillers(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(words.len());

    let mut i = 0;
    while i < words.len() {
        // Two-word phrases first, so "как бы" isn't kept as the word "как".
        let matched_phrase = i + 1 < words.len()
            && PHRASE_FILLERS
                .iter()
                .any(|phrase| core(words[i]) == phrase[0] && core(words[i + 1]) == phrase[1]);
        if matched_phrase {
            i += 2;
            continue;
        }

        let c = core(words[i]);
        if !c.is_empty() && WORD_FILLERS.contains(&c.as_str()) {
            i += 1;
            continue;
        }

        kept.push(words[i]);
        i += 1;
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
    fn removes_russian_hesitations_and_parasites() {
        assert_eq!(
            strip_fillers("ну эээ короче я пошёл домой"),
            "я пошёл домой"
        );
        assert_eq!(strip_fillers("мм значит это работает"), "это работает");
    }

    #[test]
    fn removes_two_word_phrases() {
        assert_eq!(strip_fillers("это как бы важно"), "это важно");
        assert_eq!(strip_fillers("ну в общем всё"), "всё");
    }

    #[test]
    fn removes_english_fillers() {
        assert_eq!(
            strip_fillers("um so like we should meet"),
            "so we should meet"
        );
    }

    #[test]
    fn keeps_real_words_and_tidies_punctuation() {
        // A filler carrying an attached comma is dropped whole (the comma was
        // spurious padding around the filler anyway).
        assert_eq!(strip_fillers("привет эээ, как дела"), "привет как дела");
        // A stray space before a real comma is tidied.
        assert_eq!(strip_fillers("привет ну , как дела"), "привет, как дела");
        // Sentence with no fillers is unchanged (modulo whitespace collapse).
        assert_eq!(strip_fillers("обычное предложение"), "обычное предложение");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(strip_fillers(""), "");
    }
}
