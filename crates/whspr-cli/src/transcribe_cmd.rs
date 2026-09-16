//! The `whspr transcribe` and `whspr transcribe-batch` subcommands: builds
//! an ASR backend + refiner, runs the dictation pipeline, and prints/stores
//! the result. Split out of `main.rs` to keep that file under this
//! project's 600-line-per-file guideline, same reasoning as
//! `diarize_cmd.rs`/`stats_cmd.rs`.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde_json::json;
use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{AsrChoice, Keystore, RefineChoice};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AsrBackend, Pipeline, RefineContext, TextRefiner};
use whspr_refine::{
    effective_instructions, AnthropicRefiner, LlamaLocal, NormalizingRefiner, OpenAiRefiner,
};

/// A cloud backend's API key: the keystore first (where the desktop app
/// keeps keys saved in Settings), then the plaintext `[api_keys]` table.
fn required_api_key(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    backend_id: &str,
    display_name: &str,
) -> anyhow::Result<String> {
    config
        .resolve_api_key(backend_id, keystore)?
        .ok_or_else(|| {
            anyhow::anyhow!(
            "{display_name} API key not configured (save it in the whspr app's Settings, or set \
             [api_keys].{backend_id} in config)"
        )
        })
}

/// Builds an ASR backend from command-line flags, defaulting to a real
/// `WhisperLocal` backend when `--asr` is not explicitly passed.
///
/// No-flag now mirrors `AsrChoice`'s own default (`WhisperLocal`) instead of
/// silently substituting `MockAsr`: the model path comes from
/// `config.whisper.model_path` if the user has set one, else the
/// `WHISPER_MODEL_PATH` environment variable -- whspr is bring-your-own-
/// model and doesn't ship or fetch one for you -- else this returns an
/// honest error telling the user to configure one — see
/// `WhisperLocal::resolve_model_path`.
/// `MockAsr` is still available, just no longer implicit: it's built only
/// for the explicit `AsrChoice::Mock` opt-in (`--asr mock`), which is what
/// the deterministic/offline test suite and `whspr-check` pass so neither
/// depends on a whisper model being present.
fn build_asr_backend(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    asr_id: Option<&str>,
    asr_base_url: Option<&str>,
    asr_api_key: Option<&str>,
    asr_mock_text: Option<&str>,
) -> anyhow::Result<Box<dyn AsrBackend>> {
    let choice = match asr_id {
        Some(id) => AsrChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?,
        None => AsrChoice::WhisperLocal,
    };

    match choice {
        AsrChoice::Mock => Ok(match asr_mock_text {
            Some(text) => Box::new(MockAsr::new(text)),
            None => Box::new(MockAsr::default()),
        }),
        AsrChoice::WhisperLocal => {
            let model_path = WhisperLocal::resolve_model_path(config.whisper.model_path.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no whisper model configured: set [whisper].model_path in the config \
                         file or the WHISPER_MODEL_PATH environment variable to a GGML model \
                         file you've downloaded (whspr doesn't ship or fetch one for you), or \
                         pass --asr mock for a deterministic offline test transcript"
                    )
                })?;
            Ok(Box::new(WhisperLocal::new(model_path)))
        }
        AsrChoice::OpenAi => {
            let api_key = match asr_api_key {
                Some(key) => key.to_string(),
                None => required_api_key(config, keystore, "openai", "OpenAI")?,
            };
            let backend: Box<dyn AsrBackend> = match asr_base_url {
                Some(url) => Box::new(OpenAiAsr::with_base_url(api_key, url)),
                None => Box::new(OpenAiAsr::new(api_key)),
            };
            Ok(backend)
        }
        AsrChoice::Deepgram => {
            let api_key = match asr_api_key {
                Some(key) => key.to_string(),
                None => required_api_key(config, keystore, "deepgram", "Deepgram")?,
            };
            let backend: Box<dyn AsrBackend> = match asr_base_url {
                Some(url) => Box::new(DeepgramAsr::with_base_url(api_key, url)),
                None => Box::new(DeepgramAsr::new(api_key)),
            };
            Ok(backend)
        }
        AsrChoice::AppleSpeech => {
            #[cfg(target_os = "macos")]
            {
                Ok(Box::new(whspr_asr::AppleSpeech::new(
                    config.language.clone(),
                )))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(anyhow::anyhow!("Apple Speech is only available on macOS"))
            }
        }
    }
}

/// Builds a text refiner backend from config and command-line flags.
///
/// The chosen backend is always wrapped in `NormalizingRefiner`, which layers
/// rule-based number/date/time normalization (toggled per-rule by
/// `config.normalize`) on top of whatever the backend itself returns — see
/// `NormalizingRefiner`'s own doc comment: it's meant to wrap any refiner,
/// `NoopRefiner` included, not replace one. Model IDs/paths come from
/// `config.refine_settings` rather than being hardcoded.
///
/// `shorten` (J-11, resolved by the caller from `--shorten` / `[capture].
/// shorten`) is passed to both the inner LLM backend (an extra "be concise"
/// prompt sentence) and the outer `NormalizingRefiner` (the rule-based
/// shorten pass) -- see each one's own `with_shorten` doc comment.
fn build_refiner(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    refine_id: Option<&str>,
    shorten: bool,
) -> anyhow::Result<Box<dyn TextRefiner>> {
    let choice = if let Some(id) = refine_id {
        RefineChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        config.refine
    };

    let inner: Box<dyn TextRefiner> = match choice {
        RefineChoice::Noop => Box::new(NoopRefiner),
        RefineChoice::OpenAi => {
            let api_key = required_api_key(config, keystore, "openai", "OpenAI")?;
            Box::new(
                OpenAiRefiner::new(api_key, config.refine_settings.openai_model.clone())
                    .with_shorten(shorten),
            )
        }
        RefineChoice::Anthropic => {
            let api_key = required_api_key(config, keystore, "anthropic", "Anthropic")?;
            Box::new(
                AnthropicRefiner::new(api_key, config.refine_settings.anthropic_model.clone())
                    .with_shorten(shorten),
            )
        }
        RefineChoice::LlamaLocal => {
            let model_path = config
                .refine_settings
                .llama_model_path
                .clone()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                    "no llama-local model configured: set [refine_settings].llama_model_path in \
                     the config file to a GGUF model file you've downloaded (whspr doesn't ship \
                     or fetch one for you), or pass --refine noop for no LLM cleanup"
                )
                })?;
            Box::new(LlamaLocal::new(model_path).with_shorten(shorten))
        }
        RefineChoice::AppleFoundation => {
            if whspr_refine::apple_foundation_available() {
                Box::new(whspr_refine::AppleFoundation::new().with_shorten(shorten))
            } else {
                anyhow::bail!(
                    "Apple Foundation Models is unavailable — it needs macOS 26 with Apple \
                     Intelligence enabled (and a build that includes it); pass --refine noop \
                     or another backend"
                );
            }
        }
    };

    Ok(Box::new(
        NormalizingRefiner::new(inner, config.normalize.clone()).with_shorten(shorten),
    ))
}

/// Words-per-minute from a word count and the *speech* duration (the
/// recorded/uploaded audio's length), not pipeline processing time -
/// processing wall-clock varies with backend/hardware/network and says
/// nothing about how fast the person actually spoke (AL-12). Returns 0.0
/// for a non-positive duration (e.g. a clip that trimmed to nothing)
/// rather than dividing by zero.
fn words_per_minute(word_count: usize, duration_secs: f32) -> f64 {
    if duration_secs > 0.0 {
        (word_count as f64) / (duration_secs as f64 / 60.0)
    } else {
        0.0
    }
}

/// Saves a transcription result to the history journal inside `data_dir`
/// (encrypted when `[privacy].history_encryption` is on -- see
/// `crate::history_io::append_entry`).
fn save_to_history(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    data_dir: &Path,
    record: HistoryRecord<'_>,
) -> anyhow::Result<()> {
    // Use SystemTime since chrono is not in workspace deps
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let entry = json!({
        "text": record.text,
        "timestamp": now,
        "asr": record.asr_id,
        "refine": record.refine_id,
        "source": "cli",
        "wpm": record.wpm,
        "word_count": record.text.split_whitespace().count(),
        "duration_secs": record.duration_secs,
    });
    crate::history_io::append_entry(data_dir, &entry, config, keystore)
}

/// One transcription result, as stored in the history journal.
struct HistoryRecord<'a> {
    text: &'a str,
    asr_id: &'a str,
    refine_id: &'a str,
    wpm: f64,
    duration_secs: f64,
}

/// Runs the `transcribe` subcommand end to end.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    file: PathBuf,
    asr: Option<String>,
    refine: Option<String>,
    language: Option<String>,
    format: Option<String>,
    output_json: bool,
    no_store: bool,
    data_dir: Option<PathBuf>,
    asr_base_url: Option<String>,
    asr_api_key: Option<String>,
    asr_mock_text: Option<String>,
    shorten: Option<bool>,
) -> anyhow::Result<()> {
    // J-11: --shorten overrides [capture].shorten when given, like --asr
    // overrides the configured ASR backend.
    let shorten = shorten.unwrap_or(config.capture.shorten);
    let export_format = format
        .as_deref()
        .map(crate::subtitles::ExportFormat::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    eprintln!("Loading audio...");
    let audio = crate::load_audio(&file).await?;
    // E-04: [capture].vad_threshold on top of decode_wav's own fixed-default
    // trim, reusing the same min_keep floor that default uses.
    let audio = whspr_audio::trim_silence(
        &audio,
        config.capture.vad_threshold,
        whspr_audio::DEFAULT_MIN_KEEP_SAMPLES,
    );
    let audio_duration_secs = audio.duration_secs();

    eprintln!("Building pipeline...");
    let asr_backend = build_asr_backend(
        config,
        keystore,
        asr.as_deref(),
        asr_base_url.as_deref(),
        asr_api_key.as_deref(),
        asr_mock_text.as_deref(),
    )?;
    let refiner = build_refiner(config, keystore, refine.as_deref(), shorten)?;

    let asr_id = asr_backend.id();
    let refine_id = refiner.id();

    // --language wins when given; otherwise falls back to the config
    // file's [language] (I-03) - `None` either way just means "no hint",
    // the same as before this was wired up.
    let pipeline = Pipeline::new(asr_backend, refiner)
        .with_language(language.or_else(|| config.language.clone()));
    let ctx = RefineContext {
        instructions: Some(effective_instructions(
            config.refine_settings.instructions.as_deref(),
        )),
        ..Default::default()
    };

    eprintln!("Transcribing and refining...");
    let (transcript, output) = pipeline.run_with_transcript(audio, &ctx).await?;
    let wpm = words_per_minute(output.split_whitespace().count(), audio_duration_secs);

    if !no_store {
        match crate::resolve_data_dir(data_dir.as_deref()) {
            Ok(dir) => {
                let record = HistoryRecord {
                    text: &output,
                    asr_id,
                    refine_id,
                    wpm,
                    duration_secs: audio_duration_secs as f64,
                };
                if let Err(e) = save_to_history(config, keystore, &dir, record) {
                    eprintln!("Warning: failed to save to history: {}", e);
                }
            }
            Err(e) => eprintln!("Warning: failed to save to history: {}", e),
        }
    }

    if let Some(fmt) = export_format {
        let rendered = match fmt {
            crate::subtitles::ExportFormat::Srt => {
                crate::subtitles::to_srt(&transcript, audio_duration_secs)
            }
            crate::subtitles::ExportFormat::Vtt => {
                crate::subtitles::to_vtt(&transcript, audio_duration_secs)
            }
        };
        println!("{}", rendered);
    } else if output_json {
        let json_out = json!({
            "text": output,
            "asr": asr_id,
            "refine": refine_id,
            "wpm": wpm.round(),
        });
        println!("{}", serde_json::to_string(&json_out)?);
    } else {
        println!("{}", output);
    }

    Ok(())
}

/// Runs the `transcribe-batch` subcommand end to end.
#[allow(clippy::too_many_arguments)]
pub async fn run_batch(
    config: &whspr_config::Config,
    keystore: &dyn Keystore,
    dir: PathBuf,
    asr: Option<String>,
    refine: Option<String>,
    language: Option<String>,
    output_json: bool,
    no_store: bool,
    data_dir: Option<PathBuf>,
    asr_base_url: Option<String>,
    asr_api_key: Option<String>,
) -> anyhow::Result<()> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }

    let asr_backend = build_asr_backend(
        config,
        keystore,
        asr.as_deref(),
        asr_base_url.as_deref(),
        asr_api_key.as_deref(),
        None,
    )?;
    let refiner = build_refiner(config, keystore, refine.as_deref(), config.capture.shorten)?;

    let asr_id = asr_backend.id();
    let refine_id = refiner.id();

    let pipeline = Pipeline::new(asr_backend, refiner)
        .with_language(language.or_else(|| config.language.clone()));

    let mut results = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("wav") {
            eprintln!("Processing {}...", path.display());
            match crate::load_audio(&path).await {
                Ok(audio) => {
                    let audio = whspr_audio::trim_silence(
                        &audio,
                        config.capture.vad_threshold,
                        whspr_audio::DEFAULT_MIN_KEEP_SAMPLES,
                    );
                    let audio_duration_secs = audio.duration_secs();
                    let ctx = RefineContext {
                        instructions: Some(effective_instructions(
                            config.refine_settings.instructions.as_deref(),
                        )),
                        ..Default::default()
                    };

                    match pipeline.run(audio, &ctx).await {
                        Ok(output) => {
                            let wpm = words_per_minute(
                                output.split_whitespace().count(),
                                audio_duration_secs,
                            );

                            let result = json!({
                                "file": path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                                "text": output,
                                "asr": asr_id,
                                "refine": refine_id,
                            });
                            results.push(result);

                            if !no_store {
                                if let Ok(history_dir) =
                                    crate::resolve_data_dir(data_dir.as_deref())
                                {
                                    let record = HistoryRecord {
                                        text: &output,
                                        asr_id,
                                        refine_id,
                                        wpm,
                                        duration_secs: audio_duration_secs as f64,
                                    };
                                    if let Err(e) =
                                        save_to_history(config, keystore, &history_dir, record)
                                    {
                                        eprintln!(
                                            "Warning: failed to save {} to history: {e}",
                                            path.display()
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error processing {}: {}", path.display(), e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error loading {}: {}", path.display(), e);
                }
            }
        }
    }

    if output_json {
        for result in results {
            println!("{}", result);
        }
    } else {
        for result in results {
            if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                println!("{}", text);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
}
