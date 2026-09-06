//! F-14: percents and simple fractions.
//!
//! Two related "<number> <unit-word>" patterns, both anchored on a leading
//! number (digit or spelled-out via the shared number parser):
//!   - a percent word -> "<n> %"  ("fifty percent", "пятьдесят процентов")
//!   - a fraction-denominator word -> "<n>/<d>"  ("three quarters" -> "3/4",
//!     "три четверти" -> "3/4")
//!
//! Requiring the numerator to be an explicit number keeps bare ordinals
//! ("the third day") from being mistaken for fractions.

use super::numbers::parse_number_at;
use super::split_punct;

/// True for the English/Russian words that mean "percent".
fn is_percent_word(core: &str) -> bool {
    matches!(
        core.to_lowercase().as_str(),
        "percent" | "percents" | "процент" | "процента" | "процентов"
    )
}

/// Maps a fraction-denominator word to its denominator value. Covers the
/// English ordinals plus the two Russian fraction-denominator forms that
/// actually get spoken: the feminine `-ая`/`-ья` ("одна вторая" -> 1/2) and
/// the genitive-plural `-ых`/`-их` ("две третьих" -> 2/3, "три четвёртых" ->
/// 3/4), alongside the colloquial "четверть"/"треть" quarter/third words.
fn fraction_denominator(core: &str) -> Option<u64> {
    Some(match core.to_lowercase().as_str() {
        "half" | "halves" | "половина" | "половины" | "половину" => 2,
        // No English "second(s)" here: "two seconds" is a duration, not 2/2.
        "вторая" | "вторых" => 2,
        "third" | "thirds" | "треть" | "трети" | "третей" | "третья" | "третьих" => 3,
        "quarter" | "quarters" | "fourth" | "fourths" | "четверть" | "четверти" | "четвертей"
        | "четвёртая" | "четвёртых" | "четвертая" | "четвертых" => 4,
        "fifth" | "fifths" | "пятая" | "пятых" => 5,
        "sixth" | "sixths" | "шестая" | "шестых" => 6,
        "seventh" | "sevenths" | "седьмая" | "седьмых" => 7,
        "eighth" | "eighths" | "восьмая" | "восьмых" => 8,
        "ninth" | "ninths" | "девятая" | "девятых" => 9,
        "tenth" | "tenths" | "десятая" | "десятых" => 10,
        _ => return None,
    })
}

/// Rewrites "<number> percent" as "<n> %" and "<number> <fraction-word>" as
/// "<n>/<d>", leaving everything else untouched.
pub fn normalize_percents(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((value, count)) = parse_number_at(&cores, i) {
            let unit = i + count;
            if let Some(body) = cores.get(unit).and_then(|&c| {
                if is_percent_word(c) {
                    Some(format!("{value} %"))
                } else {
                    fraction_denominator(c).map(|den| format!("{value}/{den}"))
                }
            }) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[unit]);
                out.push(format!("{prefix}{body}{suffix}"));
                i = unit + 1;
                continue;
            }
        }
        out.push(words[i].to_string());
        i += 1;
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_percent() {
        assert_eq!(normalize_percents("fifty percent"), "50 %");
        assert_eq!(normalize_percents("100 percent"), "100 %");
    }

    #[test]
    fn russian_percent() {
        assert_eq!(normalize_percents("пятьдесят процентов"), "50 %");
        assert_eq!(normalize_percents("25 процентов"), "25 %");
    }

    #[test]
    fn english_fractions() {
        assert_eq!(normalize_percents("three quarters"), "3/4");
        assert_eq!(normalize_percents("one half"), "1/2");
        assert_eq!(normalize_percents("two thirds"), "2/3");
    }

    #[test]
    fn russian_fractions() {
        assert_eq!(normalize_percents("три четверти"), "3/4");
        assert_eq!(normalize_percents("одна половина"), "1/2");
        assert_eq!(normalize_percents("пять шестых"), "5/6");
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_percents("battery at fifty percent now"),
            "battery at 50 % now"
        );
        assert_eq!(normalize_percents("(three quarters)"), "(3/4)");
    }

    #[test]
    fn leaves_non_fraction_words_untouched() {
        // A denominator word with no leading number is left alone - "third"
        // here is an ordinal, not a fraction.
        assert_eq!(normalize_percents("the third day"), "the third day");
        assert_eq!(normalize_percents("hello world"), "hello world");
        assert_eq!(normalize_percents(""), "");
    }
}
