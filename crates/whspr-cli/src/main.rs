//! whspr CLI: voice dictation via configurable ASR and text refinement backends.
//!
//! Usage:
//!   whspr transcribe <FILE|->      Transcribe an audio file (- for stdin, must be WAV format)
//!   whspr transcribe-batch <DIR>   Transcribe all .wav files in a directory
//!   whspr diarize <FILE> [--model-dir <DIR>] [--embedding <CHOICE>] [--language <LANG>] [--json]
//!                                  Diarize a multi-speaker audio file: find
//!                                  speaker turns and match them against the
//!                                  persisted speaker database
//!   whspr --version                Print version and exit
//!
//! Flags:
//!   --asr ID                        ASR backend (openai, deepgram, whisper-local, mock; default: whisper-local)
//!   --refine ID                     Text refiner (noop, openai, anthropic, llama-local; default from config)
//!   --language LANG                 BCP47 language code (e.g. en, es, fr; not yet wired to ASR)
//!   --embedding CHOICE               Speaker embedding model for `diarize` (cam-plus-plus, eres2net; default from config)
//!   --format FORMAT                 `transcribe`: timecoded export (srt, vtt); overrides --json
//!   --json                          Output JSON object instead of plain text
//!   --no-store                      Don't save result to history file

mod diarize_cmd;
mod subtitles;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use clap::{Parser, Subcommand};
use serde_json::json;
use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{api_key_for, load as load_config, AsrChoice, RefineChoice};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AsrBackend, AudioBuffer, Pipeline, RefineContext, TextRefiner};
use whspr_refine::{AnthropicRefiner, LlamaLocal, OpenAiRefiner};

#[derive(Parser)]
#[command(name = "whspr", version, about = "whspr voice dictation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Transcribe an audio file through the dictation pipeline.
    Transcribe {
        /// Path to audio file (WAV format), or - for stdin.
        file: PathBuf,

        /// ASR backend id (openai, deepgram, whisper-local, mock; default: whisper-local).
        #[arg(long)]
        asr: Option<String>,

        /// Refiner id (noop, openai, anthropic, llama-local).
        #[arg(long)]
        refine: Option<String>,

        /// BCP47 language code (e.g. en, es, fr; not yet wired to ASR).
        #[arg(long)]
        language: Option<String>,

        /// Timecoded export format ("srt" or "vtt"). When set, prints
        /// subtitle-style cues (from the ASR `Transcript`'s segment timing)
        /// instead of plain text, taking precedence over --json.
        #[arg(long)]
        format: Option<String>,

        /// Output JSON object with transcription metadata.
        #[arg(long)]
        json: bool,

        /// Don't save result to history file (privacy opt-out).
        #[arg(long)]
        no_store: bool,

        /// Override the history data directory. Hidden: test-only, so the
        /// e2e suite can redirect history writes to a tempdir instead of
        /// the real platform data dir.
        #[arg(long, hide = true)]
        data_dir: Option<PathBuf>,

        /// Override the base URL for cloud ASR backends (openai, deepgram).
        /// Hidden: test-only, so the e2e suite can point --asr openai /
        /// --asr deepgram at a wiremock::MockServer instead of the real API.
        #[arg(long, hide = true)]
        asr_base_url: Option<String>,

        /// Override the API key for cloud ASR backends, bypassing config's
        /// [api_keys] table. Hidden: test-only, so the e2e suite can drive
        /// --asr openai/deepgram without depending on a real config file
        /// being present on the machine running `cargo test`.
        #[arg(long, hide = true)]
        asr_api_key: Option<String>,
    },

    /// Transcribe all .wav files in a directory.
    TranscribeBatch {
        /// Directory path.
        dir: PathBuf,

        /// ASR backend id.
        #[arg(long)]
        asr: Option<String>,

        /// Refiner id.
        #[arg(long)]
        refine: Option<String>,

        /// BCP47 language code (not yet wired to ASR).
        #[arg(long)]
        language: Option<String>,

        /// Output JSON lines (one object per file).
        #[arg(long)]
        json: bool,

        /// Don't save results to history file.
        #[arg(long)]
        no_store: bool,

        /// Override the history data directory. Hidden: test-only, so the
        /// e2e suite can redirect history writes to a tempdir instead of
        /// the real platform data dir.
        #[arg(long, hide = true)]
        data_dir: Option<PathBuf>,

        /// Override the base URL for cloud ASR backends (openai, deepgram).
        /// Hidden: test-only, so the e2e suite can point --asr openai /
        /// --asr deepgram at a wiremock::MockServer instead of the real API.
        #[arg(long, hide = true)]
        asr_base_url: Option<String>,

        /// Override the API key for cloud ASR backends, bypassing config's
        /// [api_keys] table. Hidden: test-only, so the e2e suite can drive
        /// --asr openai/deepgram without depending on a real config file
        /// being present on the machine running `cargo test`.
        #[arg(long, hide = true)]
        asr_api_key: Option<String>,
    },

    /// Diarize a multi-speaker audio file: find speaker turns and match
    /// each one against the persisted speaker database.
    Diarize {
        /// Path to audio file (WAV format).
        file: PathBuf,

        /// Directory containing sherpa-onnx segmentation + embedding model
        /// files. Falls back to the config file's `[speaker].model-dir` if
        /// not given. If neither is set, uses a deterministic mock
        /// diarizer (offline, no real model files needed) -- same
        /// "explicit opt-in, else a safe default" philosophy as `--asr`.
        #[arg(long)]
        model_dir: Option<PathBuf>,

        /// Which speaker-embedding model to use (e.g. "cam-plus-plus",
        /// "eres2net"; see `whspr_config::SpeakerEmbeddingChoice`). Falls
        /// back to the config file's `[speaker].embedding-model` choice if
        /// not given -- never hardcoded to a single model.
        #[arg(long)]
        embedding: Option<String>,

        /// BCP47 language code (e.g. en, es, fr). Falls back to
        /// `config.language`. Accepted for consistency with `transcribe`
        /// and future use; sherpa's segmentation/embedding models are
        /// acoustic, not text-based, so diarization itself doesn't yet act
        /// on this -- it's plumbed through so a future word-level
        /// who-said-what alignment (v2) has it available from day one.
        #[arg(long)]
        language: Option<String>,

        /// Output a JSON array of `{start_secs, end_secs, speaker, score}`
        /// instead of plain text lines.
        #[arg(long)]
        json: bool,

        /// Override the data directory (speakers.json lives here). Hidden:
        /// test-only, so the e2e suite can redirect writes to a tempdir.
        #[arg(long, hide = true)]
        data_dir: Option<PathBuf>,
    },
}

/// Builds an ASR backend from command-line flags, defaulting to a real
/// `WhisperLocal` backend when `--asr` is not explicitly passed.
///
/// No-flag now mirrors `AsrChoice`'s own default (`WhisperLocal`) instead of
/// silently substituting `MockAsr`: the model path comes from
/// `config.whisper.model_path` if the user has set one, else the
/// `WHISPER_MODEL_PATH` environment variable (set automatically inside the
/// project's Nix devShell), else this returns an honest error telling the
/// user to configure one — see `WhisperLocal::resolve_model_path`.
/// `MockAsr` is still available, just no longer implicit: it's built only
/// for the explicit `AsrChoice::Mock` opt-in (`--asr mock`), which is what
/// the deterministic/offline test suite and `whspr-check` pass so neither
/// depends on a whisper model being present.
fn build_asr_backend(
    config: &whspr_config::Config,
    asr_id: Option<&str>,
    asr_base_url: Option<&str>,
    asr_api_key: Option<&str>,
) -> anyhow::Result<Box<dyn AsrBackend>> {
    let choice = match asr_id {
        Some(id) => AsrChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?,
        None => AsrChoice::WhisperLocal,
    };

    match choice {
        AsrChoice::Mock => Ok(Box::new(MockAsr::default())),
        AsrChoice::WhisperLocal => {
            let model_path = WhisperLocal::resolve_model_path(config.whisper.model_path.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no whisper model configured: set [whisper].model_path in the config \
                         file or the WHISPER_MODEL_PATH environment variable (set automatically \
                         inside `nix develop`), or pass --asr mock for a deterministic offline \
                         test transcript"
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
fn build_refiner(
    config: &whspr_config::Config,
    refine_id: Option<&str>,
) -> anyhow::Result<Box<dyn TextRefiner>> {
    let choice = if let Some(id) = refine_id {
        RefineChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        config.refine
    };

    match choice {
        RefineChoice::Noop => Ok(Box::new(NoopRefiner)),
        RefineChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                anyhow::anyhow!("OpenAI API key not configured (set [api_keys].openai in config)")
            })?;
            Ok(Box::new(OpenAiRefiner::new(api_key, "gpt-4o-mini")))
        }
        RefineChoice::Anthropic => {
            let api_key = api_key_for(config, "anthropic").ok_or_else(|| {
                anyhow::anyhow!(
                    "Anthropic API key not configured (set [api_keys].anthropic in config)"
                )
            })?;
            Ok(Box::new(AnthropicRefiner::new(
                api_key,
                "claude-3-5-sonnet-20241022",
            )))
        }
        RefineChoice::LlamaLocal => Ok(Box::new(LlamaLocal::new("model.gguf"))),
    }
}

/// Decodes an audio file from a path, or reads from stdin if path is "-".
async fn load_audio(file_path: &Path) -> anyhow::Result<AudioBuffer> {
    if file_path.to_str() == Some("-") {
        // Read WAV from stdin into a temp file
        use std::io::Read;
        let mut stdin_buf = Vec::new();
        std::io::stdin().read_to_end(&mut stdin_buf)?;

        let temp_file = tempfile::NamedTempFile::new()?;
        let temp_path = temp_file.path().to_path_buf();
        std::fs::write(&temp_path, stdin_buf)?;

        whspr_audio::decode_wav(&temp_path).map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        whspr_audio::decode_wav(file_path).map_err(|e| anyhow::anyhow!("{}", e))
    }
}

/// Resolves the directory used for the history journal. `override_dir`
/// (plumbed from the hidden `--data-dir` flag) takes precedence when set;
/// otherwise falls back to the real platform data directory.
///
/// Keeping this resolution as an explicit, injectable parameter — rather
/// than baking the `ProjectDirs` lookup directly into `save_to_history` —
/// means tests can point history writes at a `tempfile::tempdir()` instead
/// of appending to a real user's `~/.local/share/whspr` (or platform
/// equivalent) as a side effect of `cargo test`.
fn resolve_data_dir(override_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(dir) = override_dir {
        return Ok(dir.to_path_buf());
    }
    directories::ProjectDirs::from("", "", "whspr")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("cannot determine platform data dir"))
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = load_config();

    match cli.command {
        Some(Command::Transcribe {
            file,
            asr,
            refine,
            language: _language,
            format,
            json: output_json,
            no_store,
            data_dir,
            asr_base_url,
            asr_api_key,
        }) => {
            let export_format = format
                .as_deref()
                .map(subtitles::ExportFormat::from_str)
                .transpose()
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            eprintln!("Loading audio...");
            let audio = load_audio(&file).await?;
            let audio_duration_secs = audio.duration_secs();

            eprintln!("Building pipeline...");
            let asr_backend = build_asr_backend(
                &config,
                asr.as_deref(),
                asr_base_url.as_deref(),
                asr_api_key.as_deref(),
            )?;
            let refiner = build_refiner(&config, refine.as_deref())?;

            let asr_id = asr_backend.id();
            let refine_id = refiner.id();

            let pipeline = Pipeline::new(asr_backend, refiner);
            let ctx = RefineContext::default();

            eprintln!("Transcribing and refining...");
            let start = Instant::now();
            let (transcript, output) = pipeline.run_with_transcript(audio, &ctx).await?;
            let elapsed = start.elapsed().as_secs_f64();
            let wpm = (output.split_whitespace().count() as f64) / (elapsed / 60.0);

            if !no_store {
                match resolve_data_dir(data_dir.as_deref()) {
                    Ok(dir) => {
                        if let Err(e) = save_to_history(&dir, &output, asr_id, refine_id, wpm).await
                        {
                            eprintln!("Warning: failed to save to history: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Warning: failed to save to history: {}", e),
                }
            }

            if let Some(fmt) = export_format {
                let rendered = match fmt {
                    subtitles::ExportFormat::Srt => {
                        subtitles::to_srt(&transcript, audio_duration_secs)
                    }
                    subtitles::ExportFormat::Vtt => {
                        subtitles::to_vtt(&transcript, audio_duration_secs)
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
        }

        Some(Command::TranscribeBatch {
            dir,
            asr,
            refine,
            language: _language,
            json: output_json,
            no_store,
            data_dir,
            asr_base_url,
            asr_api_key,
        }) => {
            if !dir.is_dir() {
                anyhow::bail!("{} is not a directory", dir.display());
            }

            let asr_backend = build_asr_backend(
                &config,
                asr.as_deref(),
                asr_base_url.as_deref(),
                asr_api_key.as_deref(),
            )?;
            let refiner = build_refiner(&config, refine.as_deref())?;

            let asr_id = asr_backend.id();
            let refine_id = refiner.id();

            let pipeline = Pipeline::new(asr_backend, refiner);

            let mut results = Vec::new();

            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("wav") {
                    eprintln!("Processing {}...", path.display());
                    match load_audio(&path).await {
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
                                            resolve_data_dir(data_dir.as_deref())
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
        }

        Some(Command::Diarize {
            file,
            model_dir,
            embedding,
            language: _language,
            json: output_json,
            data_dir,
        }) => {
            diarize_cmd::run(&config, file, model_dir, embedding, data_dir, output_json).await?;
        }

        None => {
            anyhow::bail!(
                "no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`"
            );
        }
    }

    Ok(())
}
