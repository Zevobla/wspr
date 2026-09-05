//! Pure words-per-minute calculations for the Hub's stats section.

use crate::history::HistoryEntry;

/// Words-per-minute for a single utterance. Returns `None` for a
/// non-positive duration rather than dividing by zero or inventing a
/// number -- callers should treat that as "not enough data," not "0 wpm."
pub fn words_per_minute(word_count: usize, duration_secs: f32) -> Option<f32> {
    if duration_secs <= 0.0 {
        return None;
    }

    Some(word_count as f32 / (duration_secs / 60.0))
}

/// Average wpm across every history entry that carries a usable duration.
/// `None` (not `0.0`) when there's no such entry, so the Hub can show an
/// honest empty state instead of a fabricated number.
pub fn average_wpm(history: &[HistoryEntry]) -> Option<f32> {
    let rates: Vec<f32> = history
        .iter()
        .filter_map(|entry| words_per_minute(entry.word_count(), entry.duration_secs?))
        .collect();

    if rates.is_empty() {
        return None;
    }

    Some(rates.iter().sum::<f32>() / rates.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_per_minute_of_a_minute_long_utterance() {
        assert_eq!(words_per_minute(120, 60.0), Some(120.0));
    }

    #[test]
    fn words_per_minute_scales_shorter_utterances_up() {
        // 20 words in 10 seconds is the same pace as 120 words/minute.
        assert_eq!(words_per_minute(20, 10.0), Some(120.0));
    }

    #[test]
    fn words_per_minute_none_for_zero_duration() {
        assert_eq!(words_per_minute(10, 0.0), None);
    }

    #[test]
    fn words_per_minute_none_for_negative_duration() {
        assert_eq!(words_per_minute(10, -1.0), None);
    }

    #[test]
    fn average_wpm_none_for_empty_history() {
        assert_eq!(average_wpm(&[]), None);
    }

    #[test]
    fn average_wpm_skips_entries_without_duration() {
        let history = vec![
            HistoryEntry {
                text: "one two three four five six".to_string(), // 6 words
                duration_secs: Some(3.0),                        // 120 wpm
            },
            HistoryEntry {
                text: "no timing on this one".to_string(),
                duration_secs: None,
            },
        ];

        assert_eq!(average_wpm(&history), Some(120.0));
    }

    #[test]
    fn average_wpm_averages_multiple_entries() {
        let history = vec![
            HistoryEntry {
                text: "one two three four five six".to_string(), // 6 words / 3s = 120 wpm
                duration_secs: Some(3.0),
            },
            HistoryEntry {
                text: "one two three four".to_string(), // 4 words / 2s = 120 wpm
                duration_secs: Some(2.0),
            },
        ];

        assert_eq!(average_wpm(&history), Some(120.0));
    }
}
