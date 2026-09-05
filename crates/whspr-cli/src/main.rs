//! whspr CLI: voice dictation via configurable ASR and text refinement backends.
//!
//! Usage:
//!   whspr transcribe <FILE|->      Transcribe an audio file (- for stdin, must be WAV format)
//!   whspr transcribe-batch <DIR>   Transcribe all .wav files in a directory
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
use whspr_config::{api_key_for, load as load_config, AsrChoice, RefineChoice};
use whspr_core::testkit::NoopRefiner;
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
    },
}

/// Builds an ASR backend from config and command-line flags.
fn build_asr_backend(config: &whspr_config::Config, asr_id: Option<&str>) -> anyhow::Result<Box<dyn AsrBackend>> {
    let choice = if let Some(id) = asr_id {
        AsrChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        config.asr
    };

    match choice {
        AsrChoice::WhisperLocal => Ok(Box::new(WhisperLocal::new("model.bin"))),
        AsrChoice::OpenAi => {
            let api_key = api_key_for(config, "openai")
                .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured (set [api_keys].openai in config)"))?;
            Ok(Box::new(OpenAiAsr::new(api_key)))
        }
        AsrChoice::Deepgram => {
            let api_key = api_key_for(config, "deepgram")
                .ok_or_else(|| anyhow::anyhow!("Deepgram API key not configured (set [api_keys].deepgram in config)"))?;
            Ok(Box::new(DeepgramAsr::new(api_key)))
        }
    }
}

/// Builds a text refiner backend from config and command-line flags.
fn build_refiner(config: &whspr_config::Config, refine_id: Option<&str>) -> anyhow::Result<Box<dyn TextRefiner>> {
    let choice = if let Some(id) = refine_id {
        RefineChoice::from_str(id).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        config.refine
    };

    match choice {
        RefineChoice::Noop => Ok(Box::new(NoopRefiner)),
        RefineChoice::OpenAi => {
            let api_key = api_key_for(config, "openai")
                .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured (set [api_keys].openai in config)"))?;
            Ok(Box::new(OpenAiRefiner::new(api_key, "gpt-4o-mini")))
        }
        RefineChoice::Anthropic => {
            let api_key = api_key_for(config, "anthropic")
                .ok_or_else(|| anyhow::anyhow!("Anthropic API key not configured (set [api_keys].anthropic in config)"))?;
            Ok(Box::new(AnthropicRefiner::new(api_key, "claude-3-5-sonnet-20241022")))
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

/// Saves a transcription result to the history file.
async fn save_to_history(text: &str, asr_id: &str, refine_id: &str, wpm: f64) -> anyhow::Result<()> {
    let dirs = directories::ProjectDirs::from("", "", "whspr")
        .ok_or_else(|| anyhow::anyhow!("cannot determine platform data dir"))?;

    let data_dir = dirs.data_dir();
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

    let line = format!("{}\n", entry.to_string());
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
        }) => {
            eprintln!("Loading audio...");
            let audio = load_audio(&file).await?;

            eprintln!("Building pipeline...");
            let asr_backend = build_asr_backend(&config, asr.as_deref())?;
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
                if let Err(e) = save_to_history(&output, asr_id, refine_id, wpm).await {
                    eprintln!("Warning: failed to save to history: {}", e);
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
        }) => {
            if !dir.is_dir() {
                anyhow::bail!("{} is not a directory", dir.display());
            }

            let asr_backend = build_asr_backend(&config, asr.as_deref())?;
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
                                        let _ = save_to_history(&output, asr_id, refine_id, 0.0).await;
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
                    println!("{}", result.to_string());
                }
            } else {
                for result in results {
                    if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                        println!("{}", text);
                    }
                }
            }
        }

        None => {
            anyhow::bail!("no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`");
        }
    }

    Ok(())
}
