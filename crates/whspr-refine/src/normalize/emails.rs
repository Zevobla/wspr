//! F-16: spoken email addresses assembled into one token.
//!
//! "john dot doe at example dot com" -> "john.doe@example.com". A local part
//! and a domain part are each read as "word (dot word)*", joined by the
//! spoken "at". The domain must end in a TLD-shaped segment (2..=6 ASCII
//! letters), which keeps an ordinary "look at something" from being turned
//! into an address unless a real domain follows.

use super::split_punct;

/// True for the spoken "dot" separator (English or Russian).
fn is_dot(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "dot" | "точка")
}

/// True for the spoken "at" / "@" separator (English or Russian).
fn is_at(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "at" | "собака" | "эт")
}

/// A word usable as an address segment: alphanumeric and not one of the
/// separator keywords.
fn is_word(core: &str) -> bool {
    !core.is_empty() && core.chars().all(char::is_alphanumeric) && !is_dot(core) && !is_at(core)
}

/// A plausible top-level domain: 2..=6 ASCII letters.
fn is_tld(core: &str) -> bool {
    (2..=6).contains(&core.len()) && core.chars().all(|c| c.is_ascii_alphabetic())
}

/// Reads "word (dot word)*" starting at `i`, returning the segments and the
/// index just past the last word consumed.
fn read_dotted<'a>(cores: &[&'a str], i: usize) -> Option<(Vec<&'a str>, usize)> {
    let first = *cores.get(i)?;
    if !is_word(first) {
        return None;
    }
    let mut parts = vec![first];
    let mut k = i + 1;
    while k + 1 < cores.len() && is_dot(cores[k]) && is_word(cores[k + 1]) {
        parts.push(cores[k + 1]);
        k += 2;
    }
    Some((parts, k))
}

/// Replaces every spoken "<local> at <domain>" run with `local@domain`.
pub fn normalize_emails(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((local, k1)) = read_dotted(&cores, i) {
            if cores.get(k1).is_some_and(|&c| is_at(c)) {
                if let Some((domain, k2)) = read_dotted(&cores, k1 + 1) {
                    if domain.len() >= 2 && is_tld(domain[domain.len() - 1]) {
                        let (_, prefix, _) = split_punct(words[i]);
                        let (_, _, suffix) = split_punct(words[k2 - 1]);
                        let local = local.join(".");
                        let domain = domain.join(".");
                        out.push(format!("{prefix}{local}@{domain}{suffix}"));
                        i = k2;
                        continue;
                    }
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
    fn assembles_dotted_local_and_domain() {
        assert_eq!(
            normalize_emails("john dot doe at example dot com"),
            "john.doe@example.com"
        );
    }

    #[test]
    fn single_word_local_part() {
        assert_eq!(
            normalize_emails("contact john at example dot com please"),
            "contact john@example.com please"
        );
    }

    #[test]
    fn russian_separators_with_latin_domain() {
        assert_eq!(
            normalize_emails("иван точка петров собака mail точка ru"),
            "иван.петров@mail.ru"
        );
    }

    #[test]
    fn preserves_trailing_punctuation() {
        assert_eq!(
            normalize_emails("write to jane at example dot org."),
            "write to jane@example.org."
        );
    }

    #[test]
    fn leaves_plain_at_phrases_untouched() {
        // "at" with no dotted domain after it is not an address.
        assert_eq!(normalize_emails("meet at noon"), "meet at noon");
        assert_eq!(normalize_emails("hello world"), "hello world");
        assert_eq!(normalize_emails(""), "");
    }
}
