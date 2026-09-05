//! F-13: currency amounts written with a currency symbol.
//!
//! Recognizes "<number> <currency-word>" (English or Russian, digit or
//! spelled-out via the shared number parser) and rewrites it using the
//! currency's conventional symbol - prefixed for dollar/euro/pound ("$5"),
//! suffixed for the rouble ("100 ₽"). The amount may already be a digit run
//! (the numbers pass ran first) or still be number words, since the shared
//! `parse_number_at` accepts either.

use super::numbers::parse_number_at;
use super::split_punct;

/// Where a currency symbol sits relative to the amount.
#[derive(Clone, Copy)]
enum Placement {
    /// Symbol glued before the amount, e.g. `$5`.
    Before,
    /// Symbol after the amount with a thin space, e.g. `100 ₽`.
    After,
}

/// Maps a currency word (English or Russian, any case) to its symbol and
/// the placement convention for that symbol.
fn currency_symbol(core: &str) -> Option<(&'static str, Placement)> {
    use Placement::{After, Before};
    Some(match core.to_lowercase().as_str() {
        "dollar" | "dollars" | "usd" | "доллар" | "доллара" | "долларов" => {
            ("$", Before)
        }
        "euro" | "euros" | "евро" => ("€", Before),
        "pound" | "pounds" | "фунт" | "фунта" | "фунтов" => ("£", Before),
        "рубль" | "рубля" | "рублей" | "руб" => ("₽", After),
        _ => return None,
    })
}

/// Replaces every "<number> <currency-word>" run with the symbol form,
/// leaving surrounding punctuation on the amount / unit words in place.
pub fn normalize_currency(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((value, count)) = parse_number_at(&cores, i) {
            let unit = i + count;
            if let Some((symbol, placement)) = cores.get(unit).and_then(|&c| currency_symbol(c)) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[unit]);
                let body = match placement {
                    Placement::Before => format!("{symbol}{value}"),
                    Placement::After => format!("{value} {symbol}"),
                };
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
    fn english_prefix_currencies() {
        assert_eq!(normalize_currency("five dollars"), "$5");
        assert_eq!(normalize_currency("10 dollars"), "$10");
        assert_eq!(normalize_currency("twenty euros"), "€20");
        assert_eq!(normalize_currency("three pounds"), "£3");
    }

    #[test]
    fn russian_rouble_is_suffixed() {
        assert_eq!(normalize_currency("сто рублей"), "100 ₽");
        assert_eq!(normalize_currency("50 рублей"), "50 ₽");
        assert_eq!(normalize_currency("один рубль"), "1 ₽");
    }

    #[test]
    fn russian_prefix_currencies() {
        assert_eq!(normalize_currency("пять долларов"), "$5");
        assert_eq!(normalize_currency("десять евро"), "€10");
    }

    #[test]
    fn compound_amount() {
        assert_eq!(normalize_currency("one hundred dollars"), "$100");
        assert_eq!(normalize_currency("двести рублей"), "200 ₽");
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_currency("it costs five dollars today"),
            "it costs $5 today"
        );
        assert_eq!(normalize_currency("(five dollars)"), "($5)");
        assert_eq!(normalize_currency("цена сто рублей."), "цена 100 ₽.");
    }

    #[test]
    fn leaves_non_currency_untouched() {
        // A bare number without a following currency word is left exactly as
        // it arrived (the numbers pass, not this one, turns words to digits).
        assert_eq!(normalize_currency("five apples"), "five apples");
        assert_eq!(normalize_currency("hello world"), "hello world");
        assert_eq!(normalize_currency(""), "");
    }
}
