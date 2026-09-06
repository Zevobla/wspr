//! F-10: number words (English and Russian cardinals) written as digits.
//!
//! Supports compounds up to the millions ("one hundred and twenty five",
//! "двести тридцать" -> "230", "восемь миллионов триста сорок тысяч" ->
//! "8340000"). Scoped deliberately: no billions+, no ordinals
//! ("twenty-fifth", "двадцать пятого"). Fractions live in the percents pass.

use super::split_punct;

/// How a single number word contributes to the value being built up.
#[derive(Clone, Copy)]
enum WordKind {
    /// Added to the running accumulator: ones, teens, tens, and the
    /// irregular Russian hundreds (двести=200, триста=300, ...) which are
    /// their own words rather than a "two hundred"-style product.
    Unit(u64),
    /// Multiplies the accumulator (treating an empty accumulator as 1),
    /// and - for scales >= 1000 - flushes that product into the running
    /// total and resets the accumulator so a following "hundred" etc.
    /// starts a fresh, smaller group (e.g. "two thousand five hundred").
    Scale(u64),
}

fn word_value(word: &str) -> Option<WordKind> {
    use WordKind::{Scale, Unit};
    Some(match word {
        // English ones/teens/tens
        "zero" => Unit(0),
        "one" => Unit(1),
        "two" => Unit(2),
        "three" => Unit(3),
        "four" => Unit(4),
        "five" => Unit(5),
        "six" => Unit(6),
        "seven" => Unit(7),
        "eight" => Unit(8),
        "nine" => Unit(9),
        "ten" => Unit(10),
        "eleven" => Unit(11),
        "twelve" => Unit(12),
        "thirteen" => Unit(13),
        "fourteen" => Unit(14),
        "fifteen" => Unit(15),
        "sixteen" => Unit(16),
        "seventeen" => Unit(17),
        "eighteen" => Unit(18),
        "nineteen" => Unit(19),
        "twenty" => Unit(20),
        "thirty" => Unit(30),
        "forty" => Unit(40),
        "fifty" => Unit(50),
        "sixty" => Unit(60),
        "seventy" => Unit(70),
        "eighty" => Unit(80),
        "ninety" => Unit(90),
        "hundred" => Scale(100),
        "thousand" => Scale(1000),
        "million" => Scale(1_000_000),
        // Russian ones/teens/tens
        "ноль" => Unit(0),
        "один" | "одна" | "одно" => Unit(1),
        "два" | "две" => Unit(2),
        "три" => Unit(3),
        "четыре" => Unit(4),
        "пять" => Unit(5),
        "шесть" => Unit(6),
        "семь" => Unit(7),
        "восемь" => Unit(8),
        "девять" => Unit(9),
        "десять" => Unit(10),
        "одиннадцать" => Unit(11),
        "двенадцать" => Unit(12),
        "тринадцать" => Unit(13),
        "четырнадцать" => Unit(14),
        "пятнадцать" => Unit(15),
        "шестнадцать" => Unit(16),
        "семнадцать" => Unit(17),
        "восемнадцать" => Unit(18),
        "девятнадцать" => Unit(19),
        "двадцать" => Unit(20),
        "тридцать" => Unit(30),
        "сорок" => Unit(40),
        "пятьдесят" => Unit(50),
        "шестьдесят" => Unit(60),
        "семьдесят" => Unit(70),
        "восемьдесят" => Unit(80),
        "девяносто" => Unit(90),
        // Russian hundreds are their own irregular words, not a product.
        "сто" => Unit(100),
        "двести" => Unit(200),
        "триста" => Unit(300),
        "четыреста" => Unit(400),
        "пятьсот" => Unit(500),
        "шестьсот" => Unit(600),
        "семьсот" => Unit(700),
        "восемьсот" => Unit(800),
        "девятьсот" => Unit(900),
        "тысяча" | "тысячи" | "тысяч" => Scale(1000),
        "миллион" | "миллиона" | "миллионов" => Scale(1_000_000),
        _ => return None,
    })
}

/// Connector words that may bridge two number words ("one hundred *and*
/// five") without starting a new run on their own.
fn is_connector(word: &str) -> bool {
    matches!(word, "and" | "и")
}

/// Resolves one token to a value, falling back to splitting on '-' for
/// hyphenated English compounds ("twenty-five") that arrive as a single
/// token.
fn resolve_word(word: &str) -> Option<WordKind> {
    if let Some(k) = word_value(word) {
        return Some(k);
    }
    if word.contains('-') {
        let parts: Vec<&str> = word.split('-').collect();
        if parts.len() > 1 {
            let kinds: Option<Vec<WordKind>> = parts.iter().map(|p| word_value(p)).collect();
            if let Some(kinds) = kinds {
                return Some(WordKind::Unit(eval_kinds(&kinds)));
            }
        }
    }
    None
}

fn eval_kinds(kinds: &[WordKind]) -> u64 {
    let mut total = 0u64;
    let mut current = 0u64;
    for k in kinds {
        match *k {
            WordKind::Unit(v) => current += v,
            WordKind::Scale(v) if v >= 1000 => {
                let mult = if current == 0 { 1 } else { current };
                total += mult * v;
                current = 0;
            }
            WordKind::Scale(v) => {
                current = if current == 0 { 1 } else { current } * v;
            }
        }
    }
    total + current
}

/// Magnitude bucket of a `Unit` value, used to reject grammatically
/// invalid word orderings (see `parse_run`). Ones < teens < tens/compounds
/// < (Russian irregular) hundreds.
fn tier(v: u64) -> u8 {
    match v {
        0..=9 => 0,
        10..=19 => 1,
        20..=99 => 2,
        _ => 3,
    }
}

/// Greedily parses a run of number-words (already-stripped, lowercase-
/// insensitive `cores`) starting at `cores[0]`, allowing "and"/"и"
/// connectors between them. Returns the parsed value and how many
/// elements of `cores` it consumed, or `None` if `cores[0]` isn't a number
/// word at all.
///
/// Consecutive `Unit`s only combine when they strictly *decrease* in
/// magnitude tier (hundreds, then tens, then ones) - exactly how cardinal
/// numbers are actually spoken ("twenty five", never "five twenty"). Without
/// this check, an unrelated adjacent pair like the time "two thirty" would
/// wrongly parse as the single number 32. A `Scale` (hundred/thousand)
/// always resets the tier, since it starts a fresh, smaller group.
pub(super) fn parse_run(cores: &[&str]) -> Option<(u64, usize)> {
    let first = resolve_word(&cores.first()?.to_lowercase())?;
    let mut last_tier = match first {
        WordKind::Unit(v) => Some(tier(v)),
        WordKind::Scale(_) => None,
    };
    let mut kinds = vec![first];
    let mut j = 1;
    while let Some(&w) = cores.get(j) {
        let lower = w.to_lowercase();
        if let Some(k) = resolve_word(&lower) {
            match k {
                WordKind::Scale(_) => {
                    kinds.push(k);
                    last_tier = None;
                    j += 1;
                    continue;
                }
                WordKind::Unit(v) => {
                    let new_tier = tier(v);
                    if last_tier.is_none_or(|t| new_tier < t) {
                        kinds.push(k);
                        last_tier = Some(new_tier);
                        j += 1;
                        continue;
                    }
                }
            }
        } else if is_connector(&lower) {
            if let Some(&next) = cores.get(j + 1) {
                if resolve_word(&next.to_lowercase()).is_some() {
                    j += 1;
                    continue;
                }
            }
        }
        break;
    }
    Some((eval_kinds(&kinds), j))
}

/// Parses a number starting at `cores[i]`: a plain digit token counts as a
/// single-element run, otherwise falls back to `parse_run`. Shared with
/// `times`/`dates` so their word-based patterns work whether or not a
/// number pass already turned the surrounding words into digits.
pub(super) fn parse_number_at(cores: &[&str], i: usize) -> Option<(u64, usize)> {
    let core = *cores.get(i)?;
    if !core.is_empty() && core.chars().all(|c| c.is_ascii_digit()) {
        return core.parse().ok().map(|v| (v, 1));
    }
    parse_run(&cores[i..])
}

/// Replaces every maximal run of number words in `text` with its digit
/// value, leaving everything else - including surrounding punctuation on
/// the first/last word of a run - untouched.
pub fn normalize_numbers(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((value, count)) = parse_run(&cores[i..]) {
            let (_, prefix, _) = split_punct(words[i]);
            let (_, _, suffix) = split_punct(words[i + count - 1]);
            out.push(format!("{prefix}{value}{suffix}"));
            i += count;
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

    #[test]
    fn single_word_numbers() {
        for (word, expected) in [
            ("zero", "0"),
            ("nine", "9"),
            ("ten", "10"),
            ("fifteen", "15"),
            ("twenty", "20"),
            ("ноль", "0"),
            ("пять", "5"),
            ("десять", "10"),
            ("девятнадцать", "19"),
            ("двадцать", "20"),
        ] {
            assert_eq!(normalize_numbers(word), expected, "word: {word}");
        }
    }

    #[test]
    fn compound_tens_and_units() {
        assert_eq!(normalize_numbers("twenty five"), "25");
        assert_eq!(normalize_numbers("двадцать пять"), "25");
        assert_eq!(normalize_numbers("ninety nine"), "99");
        assert_eq!(normalize_numbers("девяносто девять"), "99");
    }

    #[test]
    fn hyphenated_compound() {
        assert_eq!(normalize_numbers("twenty-five"), "25");
    }

    #[test]
    fn rejects_invalid_word_order_as_two_separate_numbers() {
        // "two thirty" is a time ("2:30"), not the number 32 - ones can't
        // be followed by tens, so this must NOT combine into one run.
        assert_eq!(normalize_numbers("two thirty"), "2 30");
        // Two tens words in a row aren't a valid compound either.
        assert_eq!(normalize_numbers("twenty thirty"), "20 30");
    }

    #[test]
    fn hundreds_and_thousands() {
        assert_eq!(normalize_numbers("one hundred"), "100");
        assert_eq!(normalize_numbers("one hundred and five"), "105");
        assert_eq!(normalize_numbers("two hundred thirty"), "230");
        assert_eq!(normalize_numbers("двести тридцать"), "230");
        assert_eq!(normalize_numbers("two thousand twenty six"), "2026");
        assert_eq!(normalize_numbers("две тысячи двадцать шесть"), "2026");
    }

    #[test]
    fn millions_and_scale_compounds() {
        // The two cases the user reported coming out wrong.
        assert_eq!(
            normalize_numbers("восемь миллионов триста сорок тысяч"),
            "8340000"
        );
        assert_eq!(normalize_numbers("два миллиона пятьсот тысяч"), "2500000");
        // Plain place-value compounds keep working.
        assert_eq!(normalize_numbers("сто двадцать три"), "123");
        assert_eq!(normalize_numbers("тысяча девятьсот"), "1900");
        // A scale word with no leading number multiplies by one.
        assert_eq!(normalize_numbers("миллион"), "1000000");
        // English millions accumulate the same way.
        assert_eq!(
            normalize_numbers("one million two hundred thousand"),
            "1200000"
        );
    }

    #[test]
    fn regression_plain_and_non_number_text() {
        // Simple single number still normalizes.
        assert_eq!(normalize_numbers("пять"), "5");
        // A sentence with no numbers is passed through unchanged.
        assert_eq!(normalize_numbers("привет как дела"), "привет как дела");
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_numbers("I have twenty five apples."),
            "I have 25 apples."
        );
        assert_eq!(
            normalize_numbers("(twenty five)"),
            "(25)",
            "punctuation on the first/last word of a run is preserved"
        );
    }

    #[test]
    fn leaves_non_number_words_untouched() {
        assert_eq!(normalize_numbers("hello world"), "hello world");
        assert_eq!(normalize_numbers(""), "");
    }

    #[test]
    fn parse_number_at_accepts_digits_or_words() {
        assert_eq!(parse_number_at(&["14"], 0), Some((14, 1)));
        assert_eq!(parse_number_at(&["два"], 0), Some((2, 1)));
        assert_eq!(parse_number_at(&["twenty", "five"], 0), Some((25, 2)));
        assert_eq!(parse_number_at(&["hello"], 0), None);
    }
}
