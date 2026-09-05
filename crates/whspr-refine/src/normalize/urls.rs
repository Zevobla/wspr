//! F-17: spoken URLs assembled without spaces.
//!
//! "example dot com slash path" -> "example.com/path". A domain is read as
//! "word (dot word)*" and must end in a TLD-shaped segment (2..=6 ASCII
//! letters); an optional path follows as any number of "slash word" groups.
//! Runs after the email pass, so a "<local> at <domain>" address has already
//! been assembled and won't be re-read here as a bare domain.

use super::{is_tld, split_punct};

/// True for the spoken "dot" separator (English or Russian).
fn is_dot(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "dot" | "точка")
}

/// True for the spoken "slash" path separator (English or Russian).
fn is_slash(core: &str) -> bool {
    matches!(core.to_lowercase().as_str(), "slash" | "слэш" | "дробь")
}

/// A word usable as a domain or path segment: alphanumeric and not a
/// separator keyword.
fn is_word(core: &str) -> bool {
    !core.is_empty() && core.chars().all(char::is_alphanumeric) && !is_dot(core) && !is_slash(core)
}

/// Reads "word (dot word)*" starting at `i`, returning the segments and the
/// index just past the last word consumed.
fn read_domain<'a>(cores: &[&'a str], i: usize) -> Option<(Vec<&'a str>, usize)> {
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

/// Replaces every spoken domain (optionally with a slash-path) with the
/// glued-together URL form.
pub fn normalize_urls(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let cores: Vec<&str> = words.iter().map(|w| split_punct(w).0).collect();

    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((domain, after_domain)) = read_domain(&cores, i) {
            if domain.len() >= 2 && is_tld(domain[domain.len() - 1]) {
                let mut assembled = domain.join(".");
                let mut k = after_domain;
                while k + 1 < cores.len() && is_slash(cores[k]) && is_word(cores[k + 1]) {
                    assembled.push('/');
                    assembled.push_str(cores[k + 1]);
                    k += 2;
                }
                let (_, prefix, _) = split_punct(words[i]);
                let (_, _, suffix) = split_punct(words[k - 1]);
                out.push(format!("{prefix}{assembled}{suffix}"));
                i = k;
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
    fn domain_with_path() {
        assert_eq!(
            normalize_urls("example dot com slash path"),
            "example.com/path"
        );
    }

    #[test]
    fn bare_domain() {
        assert_eq!(normalize_urls("example dot com"), "example.com");
        assert_eq!(normalize_urls("www dot example dot org"), "www.example.org");
    }

    #[test]
    fn multi_segment_path() {
        assert_eq!(
            normalize_urls("example dot com slash a slash b"),
            "example.com/a/b"
        );
    }

    #[test]
    fn russian_separators() {
        assert_eq!(
            normalize_urls("сайт точка ru дробь главная"),
            "сайт.ru/главная"
        );
    }

    #[test]
    fn preserves_surrounding_text_and_punctuation() {
        assert_eq!(
            normalize_urls("visit example dot com today"),
            "visit example.com today"
        );
        assert_eq!(normalize_urls("see example dot com."), "see example.com.");
    }

    #[test]
    fn leaves_non_urls_untouched() {
        // A single dotted pair without a TLD-shaped tail is not a URL.
        assert_eq!(normalize_urls("john dot doe"), "john dot doe");
        assert_eq!(normalize_urls("hello world"), "hello world");
        assert_eq!(normalize_urls(""), "");
    }
}
