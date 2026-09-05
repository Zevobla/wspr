//! F-12: times unified to 24-hour `HH:MM`.
//!
//! Recognizes a numeric `HH<sep>MM` token (`:`, `.`, `-`, or a bare space
//! between two separate number tokens - the space form is deliberately
//! permissive per this feature's brief, at the cost of occasionally
//! matching an unrelated pair of small numbers), an optional trailing
//! am/pm, and the Russian word form "<hour> час(а/ов) [<minute>
//! (минута/минуты/минут)?]". The minute word is optional - a bare number
//! right after "N часов" is read as minutes, same trade-off as the bare
//! space-separated numeric form.

use super::numbers::parse_number_at;
use super::split_punct;

struct ParsedTime {
    hour: u64,
    minute: u64,
    /// How many `words` elements (starting at the match position) this
    /// consumed.
    consumed: usize,
}

fn split_two_numbers(core: &str, sep: char) -> Option<(u64, u64)> {
    let (a, b) = core.split_once(sep)?;
    if a.is_empty() || b.is_empty() {
        return None;
    }
    if !a.chars().all(|c| c.is_ascii_digit()) || !b.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn is_pm(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "pm" | "p.m." | "p.m")
}

fn is_am(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "am" | "a.m." | "a.m")
}

/// Applies a trailing am/pm word (if present at `cores[i]`) to `hour`,
/// following the usual 12-hour convention (12am -> 0, 12pm stays 12).
/// Returns the (possibly adjusted) hour and how many extra words were
/// consumed for the am/pm marker.
fn apply_meridiem(cores: &[&str], i: usize, hour: u64) -> (u64, usize) {
    let Some(&word) = cores.get(i) else {
        return (hour, 0);
    };
    if is_pm(word) {
        (if hour == 12 { 12 } else { hour + 12 }, 1)
    } else if is_am(word) {
        (if hour == 12 { 0 } else { hour }, 1)
    } else {
        (hour, 0)
    }
}

fn parse_numeric_at(cores: &[&str], i: usize) -> Option<ParsedTime> {
    let core = cores[i];
    for sep in [':', '.', '-'] {
        if let Some((hour, minute)) = split_two_numbers(core, sep) {
            if hour < 24 && minute < 60 {
                let (hour, extra) = apply_meridiem(cores, i + 1, hour);
                return Some(ParsedTime {
                    hour,
                    minute,
                    consumed: 1 + extra,
                });
            }
        }
    }
    // Bare "HH MM": two separate all-digit tokens.
    if !core.is_empty() && core.chars().all(|c| c.is_ascii_digit()) {
        if let Some(&next) = cores.get(i + 1) {
            if !next.is_empty() && next.chars().all(|c| c.is_ascii_digit()) {
                if let (Ok(hour), Ok(minute)) = (core.parse::<u64>(), next.parse::<u64>()) {
                    if hour < 24 && minute < 60 {
                        let (hour, extra) = apply_meridiem(cores, i + 2, hour);
                        return Some(ParsedTime {
                            hour,
                            minute,
                            consumed: 2 + extra,
                        });
                    }
                }
            }
        }
    }
    None
}

fn is_hour_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "час" | "часа" | "часов" | "o'clock" | "oclock"
    )
}

fn is_minute_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "минута" | "минуты" | "минут" | "minute" | "minutes"
    )
}

/// Replaces every recognized time expression in `text` with `HH:MM`.
pub fn normalize_times(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some(t) = parse_numeric_at(&cores, i) {
            let (_, prefix, _) = split_punct(words[i]);
            let (_, _, suffix) = split_punct(words[i + t.consumed - 1]);
            out.push(format!("{prefix}{:02}:{:02}{suffix}", t.hour, t.minute));
            i += t.consumed;
            continue;
        }
        if let Some((hour, hour_words)) = parse_number_at(&cores, i) {
            let after_hour = i + hour_words;
            if cores.get(after_hour).is_some_and(|w| is_hour_word(w)) {
                let mut minute = 0;
                let mut end = after_hour + 1;
                if let Some((m, minute_words)) = parse_number_at(&cores, end) {
                    let after_minute = end + minute_words;
                    minute = m;
                    end = if cores.get(after_minute).is_some_and(|w| is_minute_word(w)) {
                        after_minute + 1
                    } else {
                        after_minute
                    };
                }
                if hour < 24 && minute < 60 {
                    let (_, prefix, _) = split_punct(words[i]);
                    let (_, _, suffix) = split_punct(words[end - 1]);
                    out.push(format!("{prefix}{hour:02}:{minute:02}{suffix}"));
                    i = end;
                    continue;
                }
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
    fn numeric_with_separators() {
        assert_eq!(normalize_times("14:30"), "14:30");
        assert_eq!(normalize_times("14.30"), "14:30");
        assert_eq!(normalize_times("9:05"), "09:05");
    }

    #[test]
    fn numeric_bare_space() {
        assert_eq!(normalize_times("14 30"), "14:30");
    }

    #[test]
    fn numeric_with_meridiem() {
        assert_eq!(normalize_times("2:30 pm"), "14:30");
        assert_eq!(normalize_times("2:30 am"), "02:30");
        assert_eq!(normalize_times("12:00 pm"), "12:00");
        assert_eq!(normalize_times("12:00 am"), "00:00");
    }

    #[test]
    fn russian_word_form() {
        assert_eq!(normalize_times("два часа тридцать"), "02:30");
        assert_eq!(normalize_times("два часа"), "02:00");
        assert_eq!(
            normalize_times("четырнадцать часов тридцать минут"),
            "14:30"
        );
    }

    #[test]
    fn english_oclock() {
        assert_eq!(normalize_times("two o'clock"), "02:00");
    }

    #[test]
    fn preserves_surrounding_text() {
        assert_eq!(
            normalize_times("let's meet at 14:30 tomorrow"),
            "let's meet at 14:30 tomorrow"
        );
        assert_eq!(
            normalize_times("встреча в два часа тридцать вечера"),
            "встреча в 02:30 вечера"
        );
    }

    #[test]
    fn leaves_non_times_untouched() {
        assert_eq!(normalize_times("hello world"), "hello world");
        assert_eq!(
            normalize_times("25:99"),
            "25:99",
            "out of range is not a time"
        );
        assert_eq!(normalize_times(""), "");
    }
}
