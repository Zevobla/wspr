//! Ordinal numbers: "twenty fifth"/"twenty-fifth" -> "25th",
//! "двадцать пятый" -> "25-й". Deliberately narrow, matching this module's
//! existing conservatism about cardinals: only a single leading tens word
//! (twenty..ninety / двадцать..девяносто) combined with an ordinal ones
//! word (first..ninth / первый..девятый), or a standalone ordinal word
//! ("tenth" -> "10th", "hundredth" -> "100th"), is recognized. No "one
//! hundred and fifth"-style chains, and - matching the rest of this crate -
//! only the masculine nominative singular is recognized for Russian
//! ("пятый", not "пятая"/"пятого"/...), rendered with the generic "-й"
//! suffix used in ordinary Russian digit-ordinal writing.

/// Which language an ordinal word was recognized in, so the right written
/// suffix convention ("25th" vs "25-й") gets applied.
#[derive(Clone, Copy)]
enum Lang {
    En,
    Ru,
}

/// English/Russian tens words usable as the leading part of a compound
/// ordinal ("twenty" + "fifth" -> 25th). Deliberately only these eight
/// round tens - not every cardinal word makes sense before an ordinal.
fn tens_word_value(core: &str) -> Option<u64> {
    Some(match core.to_lowercase().as_str() {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        "двадцать" => 20,
        "тридцать" => 30,
        "сорок" => 40,
        "пятьдесят" => 50,
        "шестьдесят" => 60,
        "семьдесят" => 70,
        "восемьдесят" => 80,
        "девяносто" => 90,
        _ => return None,
    })
}

fn en_ordinal_word(core: &str) -> Option<u64> {
    Some(match core.to_lowercase().as_str() {
        "first" => 1,
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "fifth" => 5,
        "sixth" => 6,
        "seventh" => 7,
        "eighth" => 8,
        "ninth" => 9,
        "tenth" => 10,
        "eleventh" => 11,
        "twelfth" => 12,
        "thirteenth" => 13,
        "fourteenth" => 14,
        "fifteenth" => 15,
        "sixteenth" => 16,
        "seventeenth" => 17,
        "eighteenth" => 18,
        "nineteenth" => 19,
        "twentieth" => 20,
        "thirtieth" => 30,
        "fortieth" => 40,
        "fiftieth" => 50,
        "sixtieth" => 60,
        "seventieth" => 70,
        "eightieth" => 80,
        "ninetieth" => 90,
        "hundredth" => 100,
        "thousandth" => 1000,
        _ => return None,
    })
}

fn ru_ordinal_word(core: &str) -> Option<u64> {
    Some(match core.to_lowercase().as_str() {
        "первый" => 1,
        "второй" => 2,
        "третий" => 3,
        "четвёртый" | "четвертый" => 4,
        "пятый" => 5,
        "шестой" => 6,
        "седьмой" => 7,
        "восьмой" => 8,
        "девятый" => 9,
        "десятый" => 10,
        "одиннадцатый" => 11,
        "двенадцатый" => 12,
        "тринадцатый" => 13,
        "четырнадцатый" => 14,
        "пятнадцатый" => 15,
        "шестнадцатый" => 16,
        "семнадцатый" => 17,
        "восемнадцатый" => 18,
        "девятнадцатый" => 19,
        "двадцатый" => 20,
        "тридцатый" => 30,
        "сороковой" => 40,
        "пятидесятый" => 50,
        "шестидесятый" => 60,
        "семидесятый" => 70,
        "восьмидесятый" => 80,
        "девяностый" => 90,
        "сотый" => 100,
        "тысячный" => 1000,
        _ => return None,
    })
}

/// Resolves a single ordinal word in either language.
fn ordinal_word(core: &str) -> Option<(u64, Lang)> {
    en_ordinal_word(core)
        .map(|v| (v, Lang::En))
        .or_else(|| ru_ordinal_word(core).map(|v| (v, Lang::Ru)))
}

/// English ordinal suffix, following the standard 1st/2nd/3rd/-th rule
/// (with the 11th/12th/13th exception).
fn format_ordinal(value: u64, lang: Lang) -> String {
    match lang {
        Lang::En => {
            let suffix = if (11..=13).contains(&(value % 100)) {
                "th"
            } else {
                match value % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                }
            };
            format!("{value}{suffix}")
        }
        Lang::Ru => format!("{value}-й"),
    }
}

/// Tries to parse an ordinal number starting at `cores[i]`. Returns the
/// formatted ordinal string and how many `cores` elements it consumed, or
/// `None` if `cores[i]` doesn't start a recognized ordinal.
pub(super) fn parse_ordinal_at(cores: &[&str], i: usize) -> Option<(String, usize)> {
    let core = *cores.get(i)?;

    // A hyphenated single token, e.g. "twenty-fifth".
    if let Some((tens, ones)) = core.split_once('-') {
        if let (Some(t), Some((v, lang))) = (tens_word_value(tens), ordinal_word(ones)) {
            if (1..=9).contains(&v) {
                return Some((format_ordinal(t + v, lang), 1));
            }
        }
    }

    // A tens word followed by an ordinal ones word, e.g. "twenty fifth" /
    // "двадцать пятый".
    if let Some(t) = tens_word_value(core) {
        if let Some(&next) = cores.get(i + 1) {
            if let Some((v, lang)) = ordinal_word(next) {
                if (1..=9).contains(&v) {
                    return Some((format_ordinal(t + v, lang), 2));
                }
            }
        }
    }

    // A standalone ordinal word, e.g. "tenth" / "десятый".
    if let Some((v, lang)) = ordinal_word(core) {
        return Some((format_ordinal(v, lang), 1));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinal(text: &str) -> Option<String> {
        let words: Vec<&str> = text.split(' ').collect();
        parse_ordinal_at(&words, 0).map(|(s, count)| {
            assert_eq!(
                count,
                words.len(),
                "expected the whole input to be consumed"
            );
            s
        })
    }

    #[test]
    fn english_compound_ordinals() {
        assert_eq!(ordinal("twenty fifth"), Some("25th".to_string()));
        assert_eq!(ordinal("twenty-fifth"), Some("25th".to_string()));
        assert_eq!(ordinal("thirty first"), Some("31st".to_string()));
        assert_eq!(ordinal("forty second"), Some("42nd".to_string()));
        assert_eq!(ordinal("ninety third"), Some("93rd".to_string()));
    }

    #[test]
    fn english_standalone_ordinals() {
        assert_eq!(ordinal("fifth"), Some("5th".to_string()));
        assert_eq!(ordinal("tenth"), Some("10th".to_string()));
        assert_eq!(ordinal("eleventh"), Some("11th".to_string()));
        assert_eq!(ordinal("hundredth"), Some("100th".to_string()));
    }

    #[test]
    fn russian_compound_and_standalone_ordinals() {
        assert_eq!(ordinal("двадцать пятый"), Some("25-й".to_string()));
        assert_eq!(ordinal("тридцать первый"), Some("31-й".to_string()));
        assert_eq!(ordinal("десятый"), Some("10-й".to_string()));
        assert_eq!(ordinal("сотый"), Some("100-й".to_string()));
    }

    #[test]
    fn leaves_plain_cardinals_and_prose_alone() {
        // "twenty" alone is a cardinal, not an ordinal - no following
        // ordinal word means no match.
        assert_eq!(ordinal("twenty"), None);
        assert_eq!(ordinal("twenty five"), None);
        assert_eq!(ordinal("hello"), None);
    }
}
