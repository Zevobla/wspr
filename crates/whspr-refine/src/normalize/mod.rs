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
//!   - `dictionary` (H-01) is the same trigger -> replacement substitution
//!     as `macros` (it reuses `macros::expand_macros` directly), for
//!     finer-grained verbatim term corrections. Runs right alongside
//!     macros, before every other pass, for the same reason.
//!   - `dates`  -> dates unified to `YYYY-MM-DD`
//!   - `times`  -> times unified to 24-hour `HH:MM`
//!   - `numbers` gates the number-word pass *and* the extended token passes
//!     it feeds: currency (F-13), percents/fractions (F-14), phone numbers
//!     (F-15), emails (F-16), URLs (F-17), acronym uppercasing (F-18), and
//!     consecutive-duplicate-word collapse (F-19).
//!   - `formulas` recognizes spoken arithmetic (operator words, squared/
//!     cubed, square root of) and rewrites it as symbolic notation. Kept
//!     independent of `numbers` rather than folded into that gate, since a
//!     user may want one without the other.
//!   - `numbers_format` selects whether recognized numbers render as digits
//!     (the `numbers` pass, as above) or stay spelled out.
//!   - `punctuation_toggle` (G-16) turns spoken "comma"/"period"/"точка"/
//!     "запятая" into actual marks. Runs after the `numbers`-gated block
//!     (see its call site below for why).
//!   - `paragraph_break` (G-09) turns spoken "new paragraph"/"новый абзац"
//!     into a blank-line break. Runs dead last (see its call site below).

mod abbreviations;
mod currency;
mod dates;
mod dedup;
mod emails;
mod fillers;
mod formulas;
mod lua;
mod macros;
mod numbers;
mod paragraph;
mod percents;
mod phones;
mod punctuation;
mod times;
mod urls;

use async_trait::async_trait;
use whspr_config::{NormalizeSettings, NumberFormat};
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
/// currency pass), with the independently-toggled `formulas` pass slotted
/// in right after (so operator words claim their operands before the
/// phone-number pass could mistake a bare "2 + 3" for a space-separated
/// digit run), then emails before URLs (so an address is assembled before
/// its bare domain could be), and the duplicate-word collapse last.
pub fn apply(text: &str, settings: &NormalizeSettings) -> String {
    let mut text = macros::expand_macros(text, &settings.macros);
    // Dictionary term substitution (H-01) is the same "trigger phrase ->
    // replacement" shape as macros, so it reuses that exact matching
    // machinery (whole-word, case-insensitive, longest-trigger-first) and
    // runs right alongside it -- before any other pass has a chance to
    // rewrite a trigger term (e.g. one containing a number word) out from
    // under it, for the same reason macros itself runs first.
    text = macros::expand_macros(&text, &settings.dictionary);
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
    // `numbers_format: Words` means "keep spelled-out numbers as spelled
    // out" -- at minimum, that has to mean this pass (the one that turns a
    // bare number word into a digit) doesn't run, so "twenty five" stays
    // "twenty five" instead of becoming "25".
    if settings.numbers && settings.numbers_format == NumberFormat::Digits {
        text = numbers::normalize_numbers(&text);
    }
    // Independent of `numbers`: a user may want spoken arithmetic rewritten
    // as symbols without forcing every other bare number to render as a
    // digit, or vice versa. Runs after the plain number-word pass (so an
    // isolated operand like "two" in "two plus three" is already "2") but
    // before the `numbers`-gated block below, since it must claim its
    // operator/number runs before the phone-number heuristic in that block
    // gets a chance to eat a space-separated pair of digit tokens.
    if settings.formulas {
        text = formulas::normalize_formulas(&text);
    }
    if settings.numbers {
        text = currency::normalize_currency(&text);
        text = percents::normalize_percents(&text);
        text = phones::normalize_phones(&text);
        text = emails::normalize_emails(&text);
        text = urls::normalize_urls(&text);
        text = abbreviations::normalize_abbreviations(&text);
        text = dedup::collapse_duplicate_words(&text);
    }
    // Must run *after* the `numbers`-gated block above, specifically after
    // urls/emails: Russian "точка" means both "dot" (the URL/email
    // separator those two passes match) and "period" (the punctuation word
    // this pass matches). Running this first would consume every "точка"
    // as a period before urls/emails ever got a chance to read
    // "example точка com" as a domain.
    if settings.punctuation_toggle {
        text = punctuation::normalize_punctuation_words(&text);
    }
    // Must run dead last: every pass above tokenizes by splitting `text` on
    // `' '` and rejoining the same way, but a paragraph break is an
    // embedded `\n\n` with no surrounding space, which would otherwise fuse
    // onto its neighboring words and make them unsplittable by any pass
    // that ran afterward. See the `paragraph` module doc for the full
    // reasoning.
    if settings.paragraph_break {
        text = paragraph::normalize_paragraph_breaks(&text);
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
#[path = "mod_tests.rs"]
mod tests;
