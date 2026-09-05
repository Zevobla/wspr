/// Compute generic Levenshtein edit distance over any sequence.
///
/// Uses classic O(len_a * len_b) dynamic programming with space optimization
/// to two rows.
pub fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let m = a.len();
    let n = b.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Two rows: current and previous
    let mut prev = vec![0; n + 1];
    let mut curr = vec![0; n + 1];

    // Initialize first row (distance from empty string to prefixes of b)
    for j in 0..=n {
        prev[j] = j;
    }

    for i in 1..=m {
        curr[0] = i;

        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };

            curr[j] = std::cmp::min(
                std::cmp::min(
                    prev[j] + 1,     // deletion
                    curr[j - 1] + 1, // insertion
                ),
                prev[j - 1] + cost, // substitution
            );
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Normalize text for comparison: lowercase, strip punctuation, collapse whitespace.
fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compute Word Error Rate between hypothesis and reference.
///
/// Returns 0.0 if both are empty, 1.0 if only hypothesis is non-empty,
/// or (edit_distance / reference_word_count) otherwise.
pub fn wer(hypothesis: &str, reference: &str) -> f32 {
    let norm_hyp = normalize(hypothesis);
    let norm_ref = normalize(reference);

    let ref_words: Vec<&str> = norm_ref.split_whitespace().collect();
    let hyp_words: Vec<&str> = norm_hyp.split_whitespace().collect();

    if ref_words.is_empty() {
        return if hyp_words.is_empty() { 0.0 } else { 1.0 };
    }

    let distance = levenshtein(&hyp_words, &ref_words);
    distance as f32 / ref_words.len() as f32
}

/// Compute Character Error Rate between hypothesis and reference.
///
/// Returns 0.0 if both are empty, 1.0 if only hypothesis is non-empty,
/// or (edit_distance / reference_char_count) otherwise.
pub fn cer(hypothesis: &str, reference: &str) -> f32 {
    let norm_hyp = normalize(hypothesis);
    let norm_ref = normalize(reference);

    let ref_chars: Vec<char> = norm_ref.chars().collect();
    let hyp_chars: Vec<char> = norm_hyp.chars().collect();

    if ref_chars.is_empty() {
        return if hyp_chars.is_empty() { 0.0 } else { 1.0 };
    }

    let distance = levenshtein(&hyp_chars, &ref_chars);
    distance as f32 / ref_chars.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein(&['a', 'b', 'c'], &['a', 'b', 'c']), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein::<char>(&[], &['a', 'b']), 2);
        assert_eq!(levenshtein::<char>(&['a', 'b'], &[]), 2);
        assert_eq!(levenshtein::<char>(&[], &[]), 0);
    }

    #[test]
    fn test_levenshtein_deletion() {
        assert_eq!(levenshtein(&['a', 'b', 'c'], &['a', 'c']), 1);
    }

    #[test]
    fn test_levenshtein_insertion() {
        assert_eq!(levenshtein(&['a', 'c'], &['a', 'b', 'c']), 1);
    }

    #[test]
    fn test_levenshtein_substitution() {
        assert_eq!(levenshtein(&['a', 'x', 'c'], &['a', 'b', 'c']), 1);
    }

    #[test]
    fn test_normalize_case() {
        assert_eq!(normalize("Hello WORLD"), "hello world");
    }

    #[test]
    fn test_normalize_punctuation() {
        assert_eq!(
            normalize("Hello, world! How are you?"),
            "hello world how are you"
        );
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize("hello   world  \t  test"), "hello world test");
    }

    #[test]
    fn test_wer_identical() {
        assert_eq!(wer("hello world", "hello world"), 0.0);
    }

    #[test]
    fn test_wer_case_and_punctuation_invariant() {
        assert_eq!(
            wer("Hello, World!", "hello world"),
            0.0,
            "case and punctuation should normalize away"
        );
    }

    #[test]
    fn test_wer_one_word_deleted() {
        assert_eq!(wer("hello", "hello world"), 0.5);
    }

    #[test]
    fn test_wer_one_word_inserted() {
        assert_eq!(wer("hello world extra", "hello world"), 1.0 / 2.0);
    }

    #[test]
    fn test_wer_one_word_substituted() {
        assert_eq!(wer("hello earth", "hello world"), 0.5);
    }

    #[test]
    fn test_wer_empty_reference_empty_hypothesis() {
        assert_eq!(wer("", ""), 0.0);
    }

    #[test]
    fn test_wer_empty_reference_non_empty_hypothesis() {
        assert_eq!(wer("hello", ""), 1.0);
    }

    #[test]
    fn test_wer_empty_hypothesis_non_empty_reference() {
        assert_eq!(wer("", "hello world"), 1.0);
    }

    #[test]
    fn test_cer_identical() {
        assert_eq!(cer("hello", "hello"), 0.0);
    }

    #[test]
    fn test_cer_case_and_punctuation_invariant() {
        assert_eq!(
            cer("Hello, World!", "hello world"),
            0.0,
            "case and punctuation should normalize away"
        );
    }

    #[test]
    fn test_cer_one_char_deleted() {
        // "hello" vs "helo" -> 1 deletion, 5 chars in reference
        assert_eq!(cer("hell", "hello"), 0.2);
    }

    #[test]
    fn test_cer_empty_reference_empty_hypothesis() {
        assert_eq!(cer("", ""), 0.0);
    }

    #[test]
    fn test_cer_empty_reference_non_empty_hypothesis() {
        assert_eq!(cer("hello", ""), 1.0);
    }

    #[test]
    fn test_cer_cyrillic() {
        // Cyrillic should work the same as ASCII
        assert_eq!(cer("привет", "привет"), 0.0);
    }

    #[test]
    fn test_cer_cyrillic_one_char_deleted() {
        assert_eq!(cer("прив", "привет"), 2.0 / 6.0);
    }
}
