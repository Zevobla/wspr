//! Decimal numbers: "<int> point <digit>+" (English, digits read one at a
//! time - "three point one four" -> "3.14") and the Russian "<int> целых
//! <fraction> <denominator>" construction ("три целых четырнадцать сотых"
//! -> "3.14", read as "3 whole, 14 hundredths"), where the denominator word
//! (десятых/сотых/тысячных) fixes how many fractional digits the value is
//! zero-padded to, so "ноль целых пять тысячных" -> "0.005", not "0.5".
//!
//! Deliberately narrow: the integer part reuses the shared cardinal parser
//! (`parse_number_at`) so it accepts digit or spelled-out input the same
//! way every other pass does, but the fractional grammar itself is only
//! ever these two exact shapes - no mixing "point" with целых, no bare
//! "point" without at least one recognized digit after it.

use super::parse_number_at;

/// A single fractional digit, spoken as a word (English or Russian) or
/// already given as a digit token (also accepts a multi-digit token, e.g.
/// if a decimal's fractional part was transcribed as a bare numeral).
fn digit_chars(core: &str) -> Option<String> {
    if !core.is_empty() && core.chars().all(|c| c.is_ascii_digit()) {
        return Some(core.to_string());
    }
    let d = match core.to_lowercase().as_str() {
        "zero" | "ноль" => '0',
        "one" | "один" | "одна" | "одно" => '1',
        "two" | "два" | "две" => '2',
        "three" | "три" => '3',
        "four" | "четыре" => '4',
        "five" | "пять" => '5',
        "six" | "шесть" => '6',
        "seven" | "семь" => '7',
        "eight" | "восемь" => '8',
        "nine" | "девять" => '9',
        _ => return None,
    };
    Some(d.to_string())
}

/// True for the English decimal-point connector.
fn is_point(core: &str) -> bool {
    core.eq_ignore_ascii_case("point")
}

/// True for the Russian "N целых" (whole part) connector, in whichever
/// grammatical form ("целых" for most N, "целая" for one).
fn is_whole_connector(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "целых" | "целая" | "целое")
}

/// Maps a Russian fraction-denominator word to the number of fractional
/// digits it implies (tenths=1, hundredths=2, thousandths=3).
fn ru_denominator_digits(core: &str) -> Option<usize> {
    Some(match core.to_lowercase().as_str() {
        "десятая" | "десятых" => 1,
        "сотая" | "сотых" => 2,
        "тысячная" | "тысячных" => 3,
        _ => return None,
    })
}

/// Tries to parse a decimal number starting at `cores[i]`. Returns the
/// formatted `"int.frac"` string and how many `cores` elements it consumed,
/// or `None` if `cores[i]` doesn't start a recognized decimal.
pub(super) fn parse_decimal_at(cores: &[&str], i: usize) -> Option<(String, usize)> {
    let (int_value, int_len) = parse_number_at(cores, i)?;
    let sep = *cores.get(i + int_len)?;

    if is_point(sep) {
        let mut j = i + int_len + 1;
        let mut frac = String::new();
        while let Some(&core) = cores.get(j) {
            match digit_chars(core) {
                Some(d) => {
                    frac.push_str(&d);
                    j += 1;
                }
                None => break,
            }
        }
        if frac.is_empty() {
            return None;
        }
        return Some((format!("{int_value}.{frac}"), j - i));
    }

    if is_whole_connector(sep) {
        let frac_start = i + int_len + 1;
        let (frac_value, frac_len) = parse_number_at(cores, frac_start)?;
        let denom_idx = frac_start + frac_len;
        let digits = ru_denominator_digits(cores.get(denom_idx)?)?;
        let frac_str = format!("{frac_value:0>digits$}");
        if frac_str.len() > digits {
            // The fraction doesn't fit the denominator (e.g. "150 hundredths") -
            // not a valid decimal, leave the words alone.
            return None;
        }
        return Some((format!("{int_value}.{frac_str}"), denom_idx + 1 - i));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Option<String> {
        let words: Vec<&str> = text.split(' ').collect();
        parse_decimal_at(&words, 0).map(|(s, count)| {
            assert_eq!(count, words.len(), "expected the whole input to be consumed");
            s
        })
    }

    #[test]
    fn english_point_decimals() {
        assert_eq!(decimal("three point five"), Some("3.5".to_string()));
        assert_eq!(decimal("zero point five"), Some("0.5".to_string()));
        // Digits after "point" are read one at a time, not as a compound.
        assert_eq!(decimal("three point one four"), Some("3.14".to_string()));
    }

    #[test]
    fn english_point_preserves_leading_zeros() {
        assert_eq!(decimal("zero point zero five"), Some("0.05".to_string()));
    }

    #[test]
    fn english_point_accepts_digit_operands() {
        assert_eq!(decimal("3 point 5"), Some("3.5".to_string()));
    }

    #[test]
    fn russian_celyh_decimals() {
        assert_eq!(
            decimal("три целых четырнадцать сотых"),
            Some("3.14".to_string())
        );
        assert_eq!(
            decimal("три целых пять десятых"),
            Some("3.5".to_string())
        );
        assert_eq!(
            decimal("ноль целых пять тысячных"),
            Some("0.005".to_string()),
            "denominator fixes the zero-padded digit count"
        );
        assert_eq!(
            decimal("одна целая пять десятых"),
            Some("1.5".to_string())
        );
    }

    #[test]
    fn rejects_bare_point_with_no_digits() {
        assert_eq!(decimal("three point"), None);
    }

    #[test]
    fn rejects_fraction_too_large_for_denominator() {
        // 150 does not fit in "hundredths" (max two digits).
        assert_eq!(decimal("три целых сто пятьдесят сотых"), None);
    }

    #[test]
    fn leaves_non_decimals_alone() {
        assert_eq!(decimal("five apples"), None);
        assert_eq!(decimal("hello"), None);
    }
}
