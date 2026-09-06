//! Rule-based text normalization. Pure, deterministic string transforms - no
//! LLM, no network - toggleable via `whspr_config::NormalizeSettings`:
//!   - `macros` (AJ-01/AJ-02) expands user-defined trigger phrases into
//!     their configured expansion text -- or, for an expansion prefixed
//!     `lua:`, into the return value of a sandboxed LuaJIT script (see the
//!     `lua` module). Applied unconditionally (an empty macro table is a
//!     no-op) and *before* every other pass, so a trigger containing a
//!     number/date word (e.g. "call five") is matched against the
//!     refiner's literal output, not text the passes below have already
//!     rewritten.
//!   - `dates`  -> dates unified to `YYYY-MM-DD`
//!   - `times`  -> times unified to 24-hour `HH:MM`
//!   - `numbers` gates the number-word pass *and* the extended token passes
//!     it feeds: currency (F-13), percents/fractions (F-14), phone numbers
//!     (F-15), emails (F-16), URLs (F-17), acronym uppercasing (F-18), and
//!     consecutive-duplicate-word collapse (F-19).
//!
//! The extended passes share the existing `numbers` toggle rather than
//! introducing new config fields, so this module stays self-contained and
//! `whspr-config` is untouched.

mod abbreviations;
mod currency;
mod dates;
mod dedup;
mod emails;
mod fillers;
mod lua;
mod macros;
mod numbers;
mod percents;
mod phones;
mod times;
mod urls;

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
///
/// The extended `numbers`-gated passes then run in dependency order: the
/// number-word pass first (so "five dollars" is already "5 dollars" for the
/// currency pass), emails before URLs (so an address is assembled before its
/// bare domain could be), and the duplicate-word collapse last.
pub fn apply(text: &str, settings: &NormalizeSettings) -> String {
    let mut text = macros::expand_macros(text, &settings.macros);
    // Strip verbal fillers/disfluencies ("эээ", "ну", "короче", "um", "uh",
    // "как бы", ...) so they don't survive into the output even with the noop
    // refiner (rule-based, unlike the LLM prompt's English-only filler pass).
    text = fillers::strip_fillers(&text);
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
        text = phones::normalize_phones(&text);
        text = emails::normalize_emails(&text);
        text = urls::normalize_urls(&text);
        text = abbreviations::normalize_abbreviations(&text);
        text = dedup::collapse_duplicate_words(&text);
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

/// Whether `core` (any case) is a recognized top-level domain. Used by the
/// email and URL passes to decide that a dotted word run really is a domain.
/// A curated list is used rather than a "2..=6 letters" shape test, so an
/// ordinary phrase like "john dot doe" isn't mistaken for a `john.doe` domain.
pub(super) fn is_tld(core: &str) -> bool {
    const TLDS: &[&str] = &[
        "com", "org", "net", "edu", "gov", "mil", "int", "io", "co", "ai", "dev", "app", "me",
        "info", "biz", "name", "pro", "xyz", "site", "tech", "store", "blog", "ru", "us", "uk",
        "ca", "de", "fr", "es", "it", "nl", "jp", "cn", "in", "br", "au", "eu", "tv", "cc", "ly",
        "рф",
    ];
    TLDS.contains(&core.to_lowercase().as_str())
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
    async fn normalizing_refiner_applies_extended_numeric_passes() {
        // Proves the currency (F-13) and phone (F-15) passes run through the
        // real refiner path, and that dedup (F-19) collapses "the the".
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

        let result = refiner
            .refine(
                "the the meeting costs five dollars call 555 123 4567",
                &RefineContext::default(),
            )
            .await
            .expect("refine should succeed");

        assert_eq!(result, "the meeting costs $5 call 5551234567");
    }

    #[tokio::test]
    async fn normalizing_refiner_applies_extended_token_passes() {
        // Proves email (F-16), URL (F-17), percent (F-14) and acronym (F-18)
        // passes all run, in the right order, through the real refiner path.
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

        let result = refiner
            .refine(
                "email me at john dot doe at example dot com visit example dot com \
                 slash help fifty percent nasa",
                &RefineContext::default(),
            )
            .await
            .expect("refine should succeed");

        assert_eq!(
            result,
            "email me at john.doe@example.com visit example.com/help 50 % NASA"
        );
    }

    #[tokio::test]
    async fn normalizing_refiner_applies_macros() {
        // Proves macro expansion (AJ-01/AJ-02) runs through the real
        // refiner path, not just the pure `expand_macros` fn in isolation.
        let mut settings = NormalizeSettings::default();
        settings
            .macros
            .insert("my email".to_string(), "me@example.com".to_string());
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

        let result = refiner
            .refine("send my email please", &RefineContext::default())
            .await
            .expect("refine should succeed");

        assert_eq!(result, "send me@example.com please");
    }

    #[tokio::test]
    async fn normalizing_refiner_macros_run_before_number_normalization() {
        // A trigger containing a number word ("five") only matches if
        // macros see the refiner's literal output before the numbers pass
        // rewrites "five" to "5" -- proves the documented pass ordering.
        let mut settings = NormalizeSettings::default();
        settings
            .macros
            .insert("call five".to_string(), "5551234".to_string());
        let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

        let result = refiner
            .refine("please call five now", &RefineContext::default())
            .await
            .expect("refine should succeed");

        assert_eq!(result, "please 5551234 now");
    }

    #[tokio::test]
    async fn normalizing_refiner_respects_disabled_toggles() {
        let settings = NormalizeSettings {
            numbers: false,
            dates: false,
            times: false,
            ..Default::default()
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
