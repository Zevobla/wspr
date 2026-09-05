//! Rule-based text normalization: numbers written as digits, dates unified
//! to `YYYY-MM-DD`, times unified to 24-hour `HH:MM`. Pure, deterministic
//! string transforms - no LLM, no network - each independently toggleable
//! via `whspr_config::NormalizeSettings`.

mod currency;
mod dates;
mod numbers;
mod percents;
mod times;

use async_trait::async_trait;
use whspr_config::NormalizeSettings;
use whspr_core::{RefineContext, Result, TextRefiner};

/// Wraps any `TextRefiner` and runs the enabled normalizers over whatever
/// it returns. Normalization happens *after* the wrapped refiner, not
/// instead of it: LLM cleanup (filler removal, punctuation) and rule-based
/// normalization (digits/dates/times) are complementary passes, and the
/// wrapped refiner can be `NoopRefiner` if only the rule-based passes are
/// wanted at all.
pub struct NormalizingRefiner {
    inner: Box<dyn TextRefiner>,
    settings: NormalizeSettings,
}

impl NormalizingRefiner {
    pub fn new(inner: Box<dyn TextRefiner>, settings: NormalizeSettings) -> Self {
        Self { inner, settings }
    }
}

#[async_trait]
impl TextRefiner for NormalizingRefiner {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        let refined = self.inner.refine(raw, ctx).await?;
        Ok(apply(&refined, &self.settings))
    }

    fn id(&self) -> &'static str {
        // Delegate rather than invent a new id: from the config/pipeline's
        // point of view this is still "whichever backend was chosen", just
        // with normalization layered on top.
        self.inner.id()
    }
}

/// Runs each enabled normalizer over `text`. Dates and times run before
/// plain numbers so a phrase like "5 September 2026" or "two thirty" gets
/// matched as a whole by the more specific pattern first, instead of the
/// generic number-word pass claiming its constituent number words one at a
/// time. (Each pass also independently recognizes number words that are
/// already digits, so this order isn't load-bearing for correctness - it
/// just avoids doing the same work twice.)
pub fn apply(text: &str, settings: &NormalizeSettings) -> String {
    let mut text = text.to_string();
    if settings.dates {
        text = dates::normalize_dates(&text);
    }
    if settings.times {
        text = times::normalize_times(&text);
    }
    if settings.numbers {
        text = numbers::normalize_numbers(&text);
        text = currency::normalize_currency(&text);
        text = percents::normalize_percents(&text);
    }
    text
}

/// Splits a whitespace-delimited token into `(core, leading_punct,
/// trailing_punct)`, e.g. `"(twenty-five,"` -> `("twenty-five", "(", ",")`.
/// Used everywhere in this module so a normalizer can match against the
/// bare word while still reproducing the surrounding punctuation exactly
/// in its output.
pub(super) fn split_punct(word: &str) -> (&str, &str, &str) {
    let is_word_char = |c: char| c.is_alphanumeric();
    let core_start = word.find(is_word_char).unwrap_or(word.len());
    let (prefix, rest) = word.split_at(core_start);
    let core_end = rest
        .rfind(is_word_char)
        .map(|i| {
            i + rest[i..]
                .chars()
                .next()
                .expect("rfind found a char")
                .len_utf8()
        })
        .unwrap_or(0);
    let (core, suffix) = rest.split_at(core_end);
    (core, prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_core::WhsprError;

    struct EchoRefiner;

    #[async_trait]
    impl TextRefiner for EchoRefiner {
        async fn refine(&self, raw: &str, _ctx: &RefineContext) -> Result<String> {
            Ok(raw.to_string())
        }

        fn id(&self) -> &'static str {
            "echo"
        }
    }

    struct FailingRefiner;

    #[async_trait]
    impl TextRefiner for FailingRefiner {
        async fn refine(&self, _raw: &str, _ctx: &RefineContext) -> Result<String> {
            Err(WhsprError::Refine("boom".to_string()))
        }

        fn id(&self) -> &'static str {
            "failing"
        }
    }

    #[test]
    fn split_punct_separates_surrounding_punctuation() {
        assert_eq!(split_punct("twenty-five,"), ("twenty-five", "", ","));
        assert_eq!(split_punct("(five)"), ("five", "(", ")"));
        assert_eq!(split_punct("word"), ("word", "", ""));
        assert_eq!(split_punct("..."), ("", "...", ""));
    }

    #[tokio::test]
    async fn normalizing_refiner_applies_all_enabled_passes() {
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

        let result = refiner
            .refine(
                "meet at 14 30 on 5.9.2026, bring twenty five copies",
                &RefineContext::default(),
            )
            .await
            .expect("refine should succeed");

        assert_eq!(result, "meet at 14:30 on 2026-09-05, bring 25 copies");
    }

    #[tokio::test]
    async fn normalizing_refiner_respects_disabled_toggles() {
        let settings = NormalizeSettings {
            numbers: false,
            dates: false,
            times: false,
        };
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);
        let input = "meet at 14 30 on 5.9.2026, bring twenty five copies";

        let result = refiner
            .refine(input, &RefineContext::default())
            .await
            .expect("refine should succeed");

        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn normalizing_refiner_propagates_inner_error() {
        let refiner =
            NormalizingRefiner::new(Box::new(FailingRefiner), NormalizeSettings::default());

        let result = refiner.refine("anything", &RefineContext::default()).await;

        assert!(result.is_err());
    }

    #[test]
    fn normalizing_refiner_id_delegates_to_inner() {
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());
        assert_eq!(refiner.id(), "echo");
    }
}
