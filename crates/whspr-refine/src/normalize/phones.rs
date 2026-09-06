//! F-15: phone numbers collapsed into a single token.
//!
//! A phone number dictated as separate digit groups ("495 123 45 67") is
//! run together into one token, keeping a leading "+" if present
//! ("+7 495 123 45 67" -> "+74951234567"). The heuristic - three or more
//! adjacent all-digit groups totalling 7..=15 digits - is deliberately
//! permissive (a run like "100 200 300" would also collapse), the same
//! trade-off the times pass makes for bare space-separated numbers; dates
//! and times run first, so a real date/time is already a single token by
//! the time this pass sees the text.

use super::split_punct;

/// Number of adjacent digit groups required before a run is treated as a
/// phone number rather than an ordinary sequence of numbers.
const MIN_GROUPS: usize = 3;
/// Inclusive digit-count bounds for a plausible phone number.
const MIN_DIGITS: usize = 7;
const MAX_DIGITS: usize = 15;

fn is_digit_group(core: &str) -> bool {
    !core.is_empty() && core.chars().all(|c| c.is_ascii_digit())
}

/// Collapses runs of adjacent digit groups that look like a phone number
/// into one token, preserving a leading "+" and any trailing punctuation.
pub fn normalize_phones(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if is_digit_group(cores[i]) {
            let mut end = i;
            let mut digits = 0;
            while end < words.len() && is_digit_group(cores[end]) {
                digits += cores[end].len();
                end += 1;
            }
            let groups = end - i;
            if groups >= MIN_GROUPS && (MIN_DIGITS..=MAX_DIGITS).contains(&digits) {
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[end - 1]);
                let joined: String = cores[i..end].concat();
                out.push(format!("{prefix}{joined}{suffix}"));
                i = end;
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
    fn collapses_three_groups() {
        assert_eq!(normalize_phones("555 123 4567"), "5551234567");
    }

    #[test]
    fn collapses_with_leading_plus_and_country_code() {
        assert_eq!(normalize_phones("+7 495 123 45 67"), "+74951234567");
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_phones("call 555 123 4567 now"),
            "call 5551234567 now"
        );
        assert_eq!(
            normalize_phones("phone: 555 123 4567."),
            "phone: 5551234567."
        );
    }

    #[test]
    fn leaves_short_or_sparse_runs_untouched() {
        // Two groups is a time-like pair, not a phone number.
        assert_eq!(normalize_phones("14 30"), "14 30");
        // Three groups but too few digits total.
        assert_eq!(normalize_phones("1 2 3"), "1 2 3");
        assert_eq!(normalize_phones("hello world"), "hello world");
        assert_eq!(normalize_phones(""), "");
    }

    #[test]
    fn does_not_collapse_beyond_the_digit_ceiling() {
        // 18 digits across four groups is above the phone-number ceiling.
        assert_eq!(
            normalize_phones("123456 123456 123456 123456"),
            "123456 123456 123456 123456"
        );
    }
}
