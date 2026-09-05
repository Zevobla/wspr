//! F-11: dates unified to `YYYY-MM-DD`.
//!
//! Recognizes three shapes: a fully-numeric date with `.`/`/`/`-`
//! separators (either order - see `parse_numeric_date`), Russian
//! "<day> <month genitive> <year>", and English "<Month> <day>[,]
//! <year>". Scoped deliberately: day/year must already be digits (no
//! "the fifth of September"), and a bare two-number date with no
//! separator or month name (unlike `times`' bare "14 30") is intentionally
//! *not* matched - "5 9" is far too common a non-date phrase to guess at.

use super::split_punct;

fn ru_month(core: &str) -> Option<u32> {
    Some(match core.to_lowercase().as_str() {
        "января" => 1,
        "февраля" => 2,
        "марта" => 3,
        "апреля" => 4,
        "мая" => 5,
        "июня" => 6,
        "июля" => 7,
        "августа" => 8,
        "сентября" => 9,
        "октября" => 10,
        "ноября" => 11,
        "декабря" => 12,
        _ => return None,
    })
}

fn en_month(core: &str) -> Option<u32> {
    Some(match core.to_lowercase().as_str() {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" | "sept" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    })
}

/// A plain 1-2 digit day number, optionally with an English ordinal
/// suffix ("5th", "1st", "23rd") already attached.
fn parse_day(core: &str) -> Option<u32> {
    let digits = core.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    (1..=31).contains(&n).then_some(n)
}

fn parse_year(core: &str) -> Option<u32> {
    if core.len() == 4 && core.chars().all(|c| c.is_ascii_digit()) {
        core.parse().ok()
    } else {
        None
    }
}

/// A single token like `5.9.2026`, `05/09/2026`, or already-unified
/// `2026-09-05`. Whichever of the first/last part is 4 digits is taken as
/// the year; the other two are (day, month) in that order - i.e. this
/// assumes day-before-month when the year comes last, the common
/// Russian/European convention, not `MM/DD/YYYY`.
fn parse_numeric_date(core: &str) -> Option<String> {
    let sep = ['.', '/', '-'].into_iter().find(|&c| core.contains(c))?;
    let parts: Vec<&str> = core.split(sep).collect();
    let [a, b, c] = parts[..] else { return None };
    if ![a, b, c]
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }

    let (year, month, day): (u32, u32, u32) = if a.len() == 4 {
        (a.parse().ok()?, b.parse().ok()?, c.parse().ok()?)
    } else if c.len() == 4 {
        (c.parse().ok()?, b.parse().ok()?, a.parse().ok()?)
    } else {
        return None;
    };
    ((1..=12).contains(&month) && (1..=31).contains(&day))
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// Replaces every recognized date expression in `text` with `YYYY-MM-DD`.
pub fn normalize_dates(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some(iso) = parse_numeric_date(cores[i]) {
            let (_, prefix, suffix) = split_punct(words[i]);
            out.push(format!("{prefix}{iso}{suffix}"));
            i += 1;
            continue;
        }
        if i + 2 < words.len() {
            // Russian order: "<day> <month> <year>".
            if let (Some(day), Some(month), Some(year)) = (
                parse_day(cores[i]),
                ru_month(cores[i + 1]),
                parse_year(cores[i + 2]),
            ) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[i + 2]);
                out.push(format!("{prefix}{year:04}-{month:02}-{day:02}{suffix}"));
                i += 3;
                continue;
            }
            // English order: "<Month> <day>[,] <year>".
            if let (Some(month), Some(day), Some(year)) = (
                en_month(cores[i]),
                parse_day(cores[i + 1]),
                parse_year(cores[i + 2]),
            ) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[i + 2]);
                out.push(format!("{prefix}{year:04}-{month:02}-{day:02}{suffix}"));
                i += 3;
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
    fn numeric_day_first_with_various_separators() {
        assert_eq!(normalize_dates("5.9.2026"), "2026-09-05");
        assert_eq!(normalize_dates("05/09/2026"), "2026-09-05");
        assert_eq!(normalize_dates("5-9-2026"), "2026-09-05");
    }

    #[test]
    fn numeric_already_iso_order_is_left_unified() {
        assert_eq!(normalize_dates("2026-09-05"), "2026-09-05");
    }

    #[test]
    fn russian_day_month_year() {
        assert_eq!(normalize_dates("5 сентября 2026"), "2026-09-05");
        assert_eq!(
            normalize_dates("Встреча 1 января 2027 утром"),
            "Встреча 2027-01-01 утром"
        );
    }

    #[test]
    fn english_month_day_year() {
        assert_eq!(normalize_dates("September 5, 2026"), "2026-09-05");
        assert_eq!(normalize_dates("Sep 5 2026"), "2026-09-05");
        assert_eq!(normalize_dates("March 23rd, 2027"), "2027-03-23");
    }

    #[test]
    fn leaves_non_dates_untouched() {
        assert_eq!(normalize_dates("hello world"), "hello world");
        assert_eq!(
            normalize_dates("5 9"),
            "5 9",
            "bare numbers with no separator or month name are not a date"
        );
        assert_eq!(normalize_dates(""), "");
    }
}
