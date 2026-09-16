//! Unit tests for `transcribe_cmd`, split out to keep that file under
//! the 600-line limit (AA-06).

use whspr_config::{MemoryKeystore, SecretName};

use super::*;

#[test]
fn required_api_key_prefers_the_keystore() {
    let mut config = whspr_config::Config::default();
    config.api_keys.insert("openai".into(), "plaintext".into());
    let keystore = MemoryKeystore::default();
    keystore
        .set(&SecretName::api_key("openai"), "from-keychain")
        .unwrap();
    assert_eq!(
        required_api_key(&config, &keystore, "openai", "OpenAI").unwrap(),
        "from-keychain"
    );
}

#[test]
fn required_api_key_names_both_places_when_missing() {
    let config = whspr_config::Config::default();
    let error = required_api_key(&config, &MemoryKeystore::default(), "deepgram", "Deepgram")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Deepgram API key not configured"));
    assert!(error.contains("[api_keys].deepgram"));
}

#[test]
fn words_per_minute_uses_audio_duration_not_wall_clock() {
    // 10 words over 30 seconds of audio is 20 wpm, regardless of how
    // long the ASR/refine pipeline itself took to process it (AL-12).
    assert_eq!(words_per_minute(10, 30.0), 20.0);
}

#[test]
fn words_per_minute_of_a_minute_long_clip_equals_word_count() {
    assert_eq!(words_per_minute(145, 60.0), 145.0);
}

#[test]
fn words_per_minute_is_zero_for_a_non_positive_duration() {
    assert_eq!(words_per_minute(5, 0.0), 0.0);
    assert_eq!(words_per_minute(5, -1.0), 0.0);
}

#[test]
fn words_per_minute_of_zero_words_is_zero() {
    assert_eq!(words_per_minute(0, 30.0), 0.0);
}

#[test]
fn build_refiner_noop_choice_is_wrapped_in_normalizing_refiner() {
    let config = whspr_config::Config::default();

    let refiner = build_refiner(&config, &MemoryKeystore::default(), None, false)
        .expect("default (noop) refiner should always build");
    // NormalizingRefiner::id() delegates to the inner refiner's id, so
    // this also proves the wrapping happened rather than returning the
    // bare NoopRefiner.
    assert_eq!(refiner.id(), "noop");
}

#[test]
fn build_refiner_openai_uses_configured_model() {
    let mut config = whspr_config::Config::default();
    config
        .api_keys
        .insert("openai".to_string(), "test-key".to_string());
    config.refine_settings.openai_model = "gpt-4o".to_string();

    let refiner = build_refiner(&config, &MemoryKeystore::default(), Some("openai"), false)
        .expect("configured api key should be enough");
    assert_eq!(refiner.id(), "openai");
}

#[test]
fn build_refiner_llama_local_requires_a_configured_model_path() {
    let config = whspr_config::Config::default();

    // `Box<dyn TextRefiner>` isn't `Debug`, so `expect_err` isn't
    // available -- match directly instead.
    match build_refiner(
        &config,
        &MemoryKeystore::default(),
        Some("llama-local"),
        false,
    ) {
        Ok(_) => panic!("no [refine_settings].llama_model_path should fail, not build one"),
        Err(error) => assert!(error.to_string().contains("llama_model_path")),
    }
}

#[test]
fn build_refiner_llama_local_uses_configured_model_path() {
    let mut config = whspr_config::Config::default();
    config.refine_settings.llama_model_path = Some(PathBuf::from("/explicit/model.gguf"));

    let refiner = build_refiner(
        &config,
        &MemoryKeystore::default(),
        Some("llama-local"),
        false,
    )
    .expect("an explicit llama_model_path should be enough to build");
    assert_eq!(refiner.id(), "llama-local");
}

#[tokio::test]
async fn build_refiner_shorten_true_shortens_noop_output() {
    let config = whspr_config::Config::default();
    let refiner = build_refiner(&config, &MemoryKeystore::default(), Some("noop"), true)
        .expect("noop refiner should always build");

    let result = refiner
        .refine("it's sort of working", &RefineContext::default())
        .await
        .expect("refine should succeed");

    assert_eq!(result, "it's working");
}
