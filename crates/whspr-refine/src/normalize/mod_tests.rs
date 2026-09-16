//! Unit tests for `normalize::apply`/`NormalizingRefiner`, split out to
//! hold this crate's AA-06 line cap (same pattern as whspr-config's
//! `lib_tests.rs`).

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
async fn normalizing_refiner_applies_formulas_pass() {
    // Proves the formulas pass runs through the real refiner path, and
    // that it wins the phone-number heuristic's race for a bare
    // space-separated pair of digit tokens (see the doc comment above).
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine("two plus three equals five", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "2 + 3 = 5");
}

#[tokio::test]
async fn normalizing_refiner_respects_disabled_formulas_toggle() {
    let settings = NormalizeSettings {
        formulas: false,
        ..Default::default()
    };
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("two plus three", &RefineContext::default())
        .await
        .expect("refine should succeed");

    // Numbers still digitize (numbers stays on), but the operator word
    // is left alone since the formulas toggle is off.
    assert_eq!(result, "2 plus 3");
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
async fn normalizing_refiner_applies_dictionary_substitution() {
    // Proves the dictionary table (H-01) runs through the real refiner
    // path, reusing macros::expand_macros's word-boundary matching.
    let mut settings = NormalizeSettings::default();
    settings
        .dictionary
        .insert("wisper".to_string(), "Whspr".to_string());
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("I use wisper every day", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "I use Whspr every day");
}

#[tokio::test]
async fn normalizing_refiner_dictionary_runs_before_number_normalization() {
    // Same proof as macros' own ordering test: a dictionary trigger
    // containing a number word ("five") only matches the refiner's
    // literal output, before the numbers pass would rewrite it to "5"
    // out from under the trigger.
    let mut settings = NormalizeSettings::default();
    settings
        .dictionary
        .insert("cloud five".to_string(), "Cumulus Systems".to_string());
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("we launched cloud five today", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "we launched Cumulus Systems today");
}

#[tokio::test]
async fn normalizing_refiner_empty_dictionary_is_a_noop() {
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine("hello world", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "hello world");
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
async fn normalizing_refiner_numbers_format_words_skips_digitization() {
    // With the default `Digits` format the very first test in this file
    // (`normalizing_refiner_applies_all_enabled_passes`) already proves
    // "twenty five" becomes "25". Selecting `Words` instead must make
    // that pass a no-op, while everything else (dates/times/formulas)
    // keeps working normally.
    let settings = NormalizeSettings {
        numbers_format: NumberFormat::Words,
        ..Default::default()
    };
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("I have twenty five apples", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "I have twenty five apples");
}

#[tokio::test]
async fn normalizing_refiner_applies_punctuation_toggle() {
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine("hello comma how are you period", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "hello, how are you.");
}

#[tokio::test]
async fn normalizing_refiner_punctuation_toggle_runs_after_url_dot() {
    // Regression guard for the ordering documented at the pass's call
    // site: Russian "точка" must still assemble a URL/email when
    // `punctuation_toggle` is on (the default), not get consumed as a
    // bare "." first.
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine("сайт точка ru", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "сайт.ru");
}

#[tokio::test]
async fn normalizing_refiner_respects_disabled_punctuation_toggle() {
    let settings = NormalizeSettings {
        punctuation_toggle: false,
        ..Default::default()
    };
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("hello comma world", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "hello comma world");
}

#[tokio::test]
async fn normalizing_refiner_applies_paragraph_break() {
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine("hello new paragraph world", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "hello\n\nworld");
}

#[tokio::test]
async fn normalizing_refiner_paragraph_break_runs_after_other_passes() {
    // Proves paragraph_break running dead last doesn't break the passes
    // that ran before it: a formula and a punctuation command word on
    // either side of the break both still apply correctly.
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());

    let result = refiner
        .refine(
            "two plus three new paragraph hello comma world",
            &RefineContext::default(),
        )
        .await
        .expect("refine should succeed");

    assert_eq!(result, "2 + 3\n\nhello, world");
}

#[tokio::test]
async fn normalizing_refiner_respects_disabled_paragraph_break() {
    let settings = NormalizeSettings {
        paragraph_break: false,
        ..Default::default()
    };
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings);

    let result = refiner
        .refine("hello new paragraph world", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "hello new paragraph world");
}

#[tokio::test]
async fn normalizing_refiner_propagates_inner_error() {
    let refiner = NormalizingRefiner::new(Box::new(FailingRefiner), NormalizeSettings::default());

    let result = refiner.refine("anything", &RefineContext::default()).await;

    assert!(result.is_err());
}

#[test]
fn normalizing_refiner_id_delegates_to_inner() {
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());
    assert_eq!(refiner.id(), "echo");
}

#[tokio::test]
async fn normalizing_refiner_shorten_toggle() {
    // "sort of"/"kind of" aren't touched by any other pass here, so
    // this is a clean toggle probe: off is a no-op, on removes both.
    let text = "it's sort of working and kind of tired";
    let off = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default());
    let on = NormalizingRefiner::new(Box::new(EchoRefiner), NormalizeSettings::default())
        .with_shorten(true);
    assert_eq!(
        off.refine(text, &RefineContext::default()).await.unwrap(),
        text
    );
    assert_eq!(
        on.refine(text, &RefineContext::default()).await.unwrap(),
        "it's working and tired"
    );
}

#[tokio::test]
async fn normalizing_refiner_shorten_runs_after_macros_not_before() {
    // "kind of" is a `shorten` filler phrase; if shorten ran before
    // macro expansion it would strip "kind of" out of this trigger and
    // the macro below would never match -- proves the documented order.
    let mut settings = NormalizeSettings::default();
    settings.macros.insert(
        "call kind of urgent".to_string(),
        "lua: return 'DONE'".to_string(),
    );
    let refiner = NormalizingRefiner::new(Box::new(EchoRefiner), settings).with_shorten(true);
    let result = refiner
        .refine("call kind of urgent please", &RefineContext::default())
        .await
        .expect("refine should succeed");
    assert_eq!(result, "DONE please");
}

/// Regression coverage for the oracle's fillers-corruption review: every
/// sentence the always-on `fillers` pass used to destroy must survive the
/// default pipeline byte-for-byte, whether or not `shorten` is on.
#[test]
fn point1_corruption_examples_survive_the_default_pipeline() {
    let settings = NormalizeSettings::default();
    let cases = [
        "I like it",
        "5 мм",
        "10 м",
        "я пошёл, а он остался",
        "это значит, что",
        "сделай короче",
        "данные типа int",
        "испечь блин",
        "как бы ты поступил",
        "what I mean is",
        "вот дом",
    ];
    for input in cases {
        assert_eq!(apply(input, &settings, false), input, "input: {input:?}");
    }
}

#[test]
fn default_pipeline_still_removes_unambiguous_hesitations() {
    let settings = NormalizeSettings::default();
    assert_eq!(apply("um so", &settings, false), "so");
    assert_eq!(apply("эээ привет", &settings, false), "привет");
    assert_eq!(apply("ммм да", &settings, false), "да");
}

#[test]
fn shorten_true_drops_parenthetical_fillers_through_the_full_pipeline() {
    let settings = NormalizeSettings::default();
    assert_eq!(
        apply("ну, короче, это работает", &settings, true),
        "это работает"
    );
}

#[test]
fn shorten_true_keeps_kind_of_after_a_wh_word_through_the_full_pipeline() {
    let settings = NormalizeSettings::default();
    assert_eq!(
        apply("what kind of car", &settings, true),
        "what kind of car"
    );
}

#[test]
fn shorten_true_keeps_repeated_number_words_which_numbers_then_digitizes() {
    let settings = NormalizeSettings::default();
    // shorten's dedup guard keeps all five words (none of them collapse as
    // a stutter); the numbers pass then digitizes each independently.
    let result = apply("five five five one two", &settings, true);
    assert_eq!(result, "5 5 5 1 2");
}
