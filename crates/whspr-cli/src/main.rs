//! whspr CLI: voice dictation via configurable ASR and text refinement backends.
//!
//! Usage:
//!   whspr transcribe <FILE|->      Transcribe an audio file (- for stdin, must be WAV format)
//!   whspr transcribe-batch <DIR>   Transcribe all .wav files in a directory
//!   whspr diarize <FILE> [--model-dir <DIR>] [--embedding <CHOICE>] [--language <LANG>] [--json]
//!                                  Diarize a multi-speaker audio file: find
//!                                  speaker turns and match them against the
//!                                  persisted speaker database
//!   whspr stats [--csv]            Print per-utterance stats from the history journal
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
//!   --csv                           `stats`: output CSV instead of a human-readable table

mod diarize_cmd;
mod stats_cmd;
mod subtitles;
mod transcribe_cmd;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use whspr_config::load as load_config;
use whspr_core::AudioBuffer;

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

        /// Override the canned transcript `--asr mock` returns. Hidden:
        /// test-only, so the e2e suite can drive a normalizable phrase
        /// through the real transcribe path without disturbing every other
        /// test's fixed expectation of MockAsr's default text.
        #[arg(long, hide = true)]
        asr_mock_text: Option<String>,
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

    /// Print per-utterance statistics (wpm, word count, ...) from the
    /// history journal (`history.jsonl`, written by `save_to_history`).
    Stats {
        /// Output as CSV instead of a human-readable table.
        #[arg(long)]
        csv: bool,

        /// Override the history data directory. Hidden: test-only, so the
        /// e2e suite can point at a tempdir instead of the real platform
        /// data dir.
        #[arg(long, hide = true)]
        data_dir: Option<PathBuf>,
    },
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = load_config();

    match cli.command {
        Some(Command::Transcribe {
            file,
            asr,
            refine,
            language,
            format,
            json: output_json,
            no_store,
            data_dir,
            asr_base_url,
            asr_api_key,
            asr_mock_text,
        }) => {
            transcribe_cmd::run(
                &config,
                file,
                asr,
                refine,
                language,
                format,
                output_json,
                no_store,
                data_dir,
                asr_base_url,
                asr_api_key,
                asr_mock_text,
            )
            .await?;
        }

        Some(Command::TranscribeBatch {
            dir,
            asr,
            refine,
            language,
            json: output_json,
            no_store,
            data_dir,
            asr_base_url,
            asr_api_key,
        }) => {
            transcribe_cmd::run_batch(
                &config,
                dir,
                asr,
                refine,
                language,
                output_json,
                no_store,
                data_dir,
                asr_base_url,
                asr_api_key,
            )
            .await?;
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

        Some(Command::Stats { csv, data_dir }) => {
            stats_cmd::run(data_dir, csv).await?;
        }

        None => {
            anyhow::bail!(
                "no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`"
            );
        }
    }

    Ok(())
}
