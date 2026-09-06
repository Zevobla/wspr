//! The `whspr transcribe` and `whspr transcribe-batch` subcommands: builds
//! an ASR backend + refiner, runs the dictation pipeline, and prints/stores
//! the result. Split out of `main.rs` to keep that file under this
//! project's 600-line-per-file guideline, same reasoning as
//! `diarize_cmd.rs`/`stats_cmd.rs`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use serde_json::json;
use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{api_key_for, AsrChoice, RefineChoice};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AsrBackend, Pipeline, RefineContext, TextRefiner};
use whspr_refine::{AnthropicRefiner, LlamaLocal, NormalizingRefiner, OpenAiRefiner};

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
                None => api_key_for(config, "openai").ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenAI API key not configured (set [api_keys].openai in config)"
                    )
                })?,
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
                None => api_key_for(config, "deepgram").ok_or_else(|| {
                    anyhow::anyhow!(
                        "Deepgram API key not configured (set [api_keys].deepgram in config)"
                    )
                })?,
            };
            let backend: Box<dyn AsrBackend> = match asr_base_url {
                Some(url) => Box::new(DeepgramAsr::with_base_url(api_key, url)),
                None => Box::new(DeepgramAsr::new(api_key)),
            };
            Ok(backend)
        }
    }
}

/// Builds a text refiner backend from config and command-line flags.
///
/// The chosen backend is always wrapped in `NormalizingRefiner`, which layers
/// rule-based number/date/time normalization (toggled per-rule by
/// `config.normalize`) on top of whatever the backend itself returns — see
/// `NormalizingRefiner`'s own doc comment: it's meant to wrap any refiner,
/// `NoopRefiner` included, not replace one.
fn build_refiner(
    config: &whspr_config::Config,
    refine_id: Option<&str>,
) -> anyhow::Result<Box<dyn TextRefiner>> {
    let choice = if let Some(id) = refine_id {
        RefineChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        config.refine
    };

    let inner: Box<dyn TextRefiner> = match choice {
        RefineChoice::Noop => Box::new(NoopRefiner),
        RefineChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                anyhow::anyhow!("OpenAI API key not configured (set [api_keys].openai in config)")
            })?;
            Box::new(OpenAiRefiner::new(api_key, "gpt-4o-mini"))
        }
        RefineChoice::Anthropic => {
            let api_key = api_key_for(config, "anthropic").ok_or_else(|| {
                anyhow::anyhow!(
                    "Anthropic API key not configured (set [api_keys].anthropic in config)"
                )
            })?;
            Box::new(AnthropicRefiner::new(api_key, "claude-3-5-sonnet-20241022"))
        }
        RefineChoice::LlamaLocal => Box::new(LlamaLocal::new("model.gguf")),
    };

    Ok(Box::new(NormalizingRefiner::new(inner, config.normalize)))
}

/// Saves a transcription result to `history.jsonl` inside `data_dir`.
async fn save_to_history(
    data_dir: &Path,
    text: &str,
    asr_id: &str,
    refine_id: &str,
    wpm: f64,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir)?;

    let history_path = data_dir.join("history.jsonl");
    let word_count = text.split_whitespace().count();

    // Use SystemTime since chrono is not in workspace deps
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let entry = json!({
        "text": text,
        "timestamp": now,
        "asr": asr_id,
        "refine": refine_id,
        "source": "cli",
        "wpm": wpm,
        "word_count": word_count,
    });

    let line = format!("{}\n", entry);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path)?
        .write_all(line.as_bytes())?;

    Ok(())
}

/// Runs the `transcribe` subcommand end to end.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &whspr_config::Config,
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
) -> anyhow::Result<()> {
    let export_format = format
        .as_deref()
        .map(crate::subtitles::ExportFormat::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    eprintln!("Loading audio...");
    let audio = crate::load_audio(&file).await?;
    let audio_duration_secs = audio.duration_secs();

    eprintln!("Building pipeline...");
    let asr_backend = build_asr_backend(
        config,
        asr.as_deref(),
        asr_base_url.as_deref(),
        asr_api_key.as_deref(),
        asr_mock_text.as_deref(),
    )?;
    let refiner = build_refiner(config, refine.as_deref())?;

    let asr_id = asr_backend.id();
    let refine_id = refiner.id();

    // --language wins when given; otherwise falls back to the config
    // file's [language] (I-03) - `None` either way just means "no hint",
    // the same as before this was wired up.
    let pipeline = Pipeline::new(asr_backend, refiner)
        .with_language(language.or_else(|| config.language.clone()));
    let ctx = RefineContext::default();

    eprintln!("Transcribing and refining...");
    let start = Instant::now();
    let (transcript, output) = pipeline.run_with_transcript(audio, &ctx).await?;
    let elapsed = start.elapsed().as_secs_f64();
    let wpm = (output.split_whitespace().count() as f64) / (elapsed / 60.0);

    if !no_store {
        match crate::resolve_data_dir(data_dir.as_deref()) {
            Ok(dir) => {
                if let Err(e) = save_to_history(&dir, &output, asr_id, refine_id, wpm).await {
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
        asr.as_deref(),
        asr_base_url.as_deref(),
        asr_api_key.as_deref(),
        None,
    )?;
    let refiner = build_refiner(config, refine.as_deref())?;

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
                    let ctx = RefineContext::default();

                    match pipeline.run(audio, &ctx).await {
                        Ok(output) => {
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
                                    let _ = save_to_history(
                                        &history_dir,
                                        &output,
                                        asr_id,
                                        refine_id,
                                        0.0,
                                    )
                                    .await;
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
