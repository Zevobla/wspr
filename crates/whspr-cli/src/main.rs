//! whspr CLI: voice dictation via configurable ASR and text refinement backends.
//!
//! Usage:
//!   whspr transcribe <FILE|->      Transcribe an audio file (- for stdin, must be WAV format)
//!   whspr transcribe-batch <DIR>   Transcribe all .wav files in a directory
//!   whspr diarize <FILE>           Diarize a multi-speaker audio file: find
//!                                  speaker turns and match them against the
//!                                  persisted speaker database
//!   whspr --version                Print version and exit
//!
//! Flags:
//!   --asr ID                        ASR backend (openai, deepgram, whisper-local; default from config)
//!   --refine ID                     Text refiner (noop, openai, anthropic, llama-local; default from config)
//!   --language LANG                 BCP47 language code (e.g. en, es, fr; not yet wired to ASR)
//!   --json                          Output JSON object instead of plain text
//!   --no-store                      Don't save result to history file

use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use clap::{Parser, Subcommand};
use serde_json::json;
use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{api_key_for, load as load_config, AsrChoice, RefineChoice, SpeakerDb};
use whspr_core::testkit::{MockAsr, MockDiarizer, NoopRefiner};
use whspr_core::{AsrBackend, AudioBuffer, Diarizer, Pipeline, RefineContext, TextRefiner};
use whspr_diarize::SherpaDiarizer;
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

        /// ASR backend id (openai, deepgram, whisper-local).
        #[arg(long)]
        asr: Option<String>,

        /// Refiner id (noop, openai, anthropic, llama-local).
        #[arg(long)]
        refine: Option<String>,

        /// BCP47 language code (e.g. en, es, fr; not yet wired to ASR).
        #[arg(long)]
        language: Option<String>,

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

/// Builds an ASR backend from command-line flags, falling back to `MockAsr`
/// when `--asr` is not explicitly passed.
///
/// This deliberately does *not* fall back to `config.asr` the way
/// `build_refiner` falls back to `config.refine`. `RefineChoice`'s default
/// (`Noop`, via `NoopRefiner`) is a real, always-available backend, so
/// honoring it as an implicit default is safe. `AsrChoice`'s default
/// (`WhisperLocal`) is not: it's indistinguishable from "nothing configured"
/// (there's no `AsrChoice::Unset`, and no way to tell "the user's config
/// file explicitly said whisper-local" apart from "no config file at all"),
/// and `WhisperLocal` requires a whisper-rs build (cmake) that isn't
/// available in the default dev environment. If we honored `config.asr`
/// here, the CLI's most basic invocation — `whspr transcribe <file>` with no
/// flags at all — would silently try to build a real `WhisperLocal` backend
/// and fail immediately. Defaulting to `MockAsr` keeps the no-flag path
/// deterministic and always available offline; a real backend is only
/// constructed when the user explicitly opts in via `--asr <id>`.
fn build_asr_backend(
    config: &whspr_config::Config,
    asr_id: Option<&str>,
    asr_base_url: Option<&str>,
    asr_api_key: Option<&str>,
) -> anyhow::Result<Box<dyn AsrBackend>> {
    let choice = match asr_id {
        Some(id) => AsrChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?,
        None => return Ok(Box::new(MockAsr::default())),
    };

    match choice {
        AsrChoice::WhisperLocal => Ok(Box::new(WhisperLocal::new("model.bin"))),
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

/// Builds a diarization backend from config and command-line flags, falling
/// back to `MockDiarizer` when neither `--model-dir` nor the config file's
/// `[speaker].model_dir` is set -- mirrors `build_asr_backend`'s "explicit
/// opt-in, else a deterministic default" reasoning: a real `SherpaDiarizer`
/// needs model files that aren't guaranteed present, so it's never
/// constructed unless the user pointed at a model directory somehow.
fn build_diarizer(
    config: &whspr_config::Config,
    model_dir_flag: Option<&Path>,
) -> anyhow::Result<Box<dyn Diarizer>> {
    let model_dir = model_dir_flag
        .map(PathBuf::from)
        .or_else(|| config.speaker.model_dir.clone());

    match model_dir {
        Some(dir) => {
            let diarizer = SherpaDiarizer::new(&dir).map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok(Box::new(diarizer))
        }
        None => Ok(Box::new(MockDiarizer::default())),
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
            json: output_json,
            no_store,
            data_dir,
            asr_base_url,
            asr_api_key,
        }) => {
            eprintln!("Loading audio...");
            let audio = load_audio(&file).await?;

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
            let output = pipeline.run(audio, &ctx).await?;
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

            if output_json {
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
            json: output_json,
            data_dir,
        }) => {
            eprintln!("Loading audio...");
            let audio = load_audio(&file).await?;
            let audio =
                whspr_audio::resample_to_16k_mono(&audio).map_err(|e| anyhow::anyhow!("{}", e))?;

            let diarizer = build_diarizer(&config, model_dir.as_deref())?;

            eprintln!("Running diarization...");
            let turns = diarizer.diarize(&audio).map_err(|e| anyhow::anyhow!("{}", e))?;

            let data_dir = resolve_data_dir(data_dir.as_deref())?;
            std::fs::create_dir_all(&data_dir)?;
            let speakers_path = data_dir.join("speakers.json");
            let mut speaker_db = SpeakerDb::load(&speakers_path);

            let scan_id = file.display().to_string();
            let threshold = config.speaker.similarity_threshold;
            let labeled_turns: Vec<_> = turns
                .into_iter()
                .map(|mut turn| {
                    let (id, _is_new) =
                        speaker_db.match_or_enroll(&turn.embedding, threshold, &scan_id);
                    turn.speaker = Some(id);
                    turn
                })
                .collect();

            speaker_db
                .save(&speakers_path)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            if output_json {
                let json_out: Vec<_> = labeled_turns
                    .iter()
                    .map(|t| {
                        json!({
                            "start_secs": t.start_secs,
                            "end_secs": t.end_secs,
                            "speaker": t.speaker,
                            "score": t.score,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string(&json_out)?);
            } else {
                for t in &labeled_turns {
                    println!(
                        "[{:.2}-{:.2}] {}",
                        t.start_secs,
                        t.end_secs,
                        t.speaker.as_deref().unwrap_or("?")
                    );
                }
            }
        }

        None => {
            anyhow::bail!(
                "no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`"
            );
        }
    }

    Ok(())
}
