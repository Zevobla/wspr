//! whspr CLI: voice dictation via configurable ASR and text refinement backends.
//!
//! Usage:
//!   whspr transcribe <FILE|->      Transcribe an audio file (- for stdin, must be WAV format)
//!   whspr transcribe-batch <DIR>   Transcribe all .wav files in a directory
//!   whspr diarize <FILE> [--model-dir <DIR>] [--embedding <CHOICE>] [--language <LANG>] [--json]
//!                                  Diarize a multi-speaker audio file: find
//!                                  speaker turns and match them against the
//!                                  persisted speaker database
//!   whspr stats [--csv] [--by-backend] [--clear]
//!                                  Print (or clear) per-utterance stats from the history journal
//!   whspr uninstall [--yes]        Remove the autostart entry, keychain entries and config/data directories
//!   whspr --version                Print version and exit
//!
//! Flags:
//!   --asr ID                        ASR backend (openai, deepgram, whisper-local, mock; default: whisper-local)
//!   --refine ID                     Text refiner (noop, openai, anthropic, llama-local; default from config)
//!   --language LANG                 BCP47 language code for transcribe/transcribe-batch (e.g. en, es, fr; default: config.language)
//!   --embedding CHOICE               Speaker embedding model for `diarize` (cam-plus-plus, eres2net; default from config)
//!   --format FORMAT                 `transcribe`: timecoded export (srt, vtt); overrides --json
//!   --shorten [true|false]           `transcribe`: rule-based filler/repetition shortening (J-11); overrides [capture].shorten
//!   --json                          Output JSON object instead of plain text
//!   --no-store                      Don't save result to history file
//!   --csv                           `stats`: output CSV instead of a human-readable table
//!   --by-backend                    `stats`: group output by (asr, refine) backend pair
//!   --clear                         `stats`: wipe the stored history instead of printing it
//!   --yes                           `uninstall`: actually perform the removal (a dry run otherwise)

mod diarize_cmd;
mod history_io;
mod stats_cmd;
mod subtitles;
mod transcribe_cmd;
mod uninstall_cmd;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use whspr_config::load as load_config;
use whspr_config::{Keystore, MemoryKeystore, OsKeystore};
use whspr_core::AudioBuffer;

#[derive(Parser)]
#[command(name = "whspr", version, about = "whspr voice dictation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Override the config directory whspr reads `config.toml` from (and
    /// first-run-writes defaults into), routing through
    /// `whspr_config::load_from` instead of `load()`. Hidden: test-only,
    /// so the e2e suite can point every invocation at an isolated tempdir
    /// instead of the real platform config dir -- whose developer-specific
    /// settings (e.g. a non-default `refine` backend) would otherwise make
    /// tests slow or nondeterministic. `global = true` so it parses
    /// whether given before or after the subcommand, same as any other
    /// clap global flag.
    #[arg(long = "config-dir", hide = true, global = true)]
    config_dir: Option<PathBuf>,
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

        /// BCP47 language code passed to the ASR backend (e.g. en, es,
        /// fr). Falls back to `config.language` if not given.
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

        /// Apply the rule-based shorten pass (J-11): drops filler phrases
        /// and immediate word repetitions, and asks LLM refiners to be
        /// concise too. Overrides `[capture].shorten` when given (true or
        /// false), the same way --asr overrides the configured ASR
        /// backend; falls back to config when not passed.
        #[arg(long)]
        shorten: Option<bool>,
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

        /// BCP47 language code passed to the ASR backend. Falls back to
        /// `config.language` if not given.
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

        /// Wipe the stored history instead of printing it (T-09).
        #[arg(long)]
        clear: bool,

        /// Group output by (asr, refine) backend pair - count, average
        /// wpm, and total words per pair - instead of one row per
        /// utterance (T-09).
        #[arg(long)]
        by_backend: bool,

        /// Override the history data directory. Hidden: test-only, so the
        /// e2e suite can point at a tempdir instead of the real platform
        /// data dir.
        #[arg(long, hide = true)]
        data_dir: Option<PathBuf>,
    },

    /// Remove the autostart entry and whspr's config/data directories
    /// (AH-08). A dry run by default - pass --yes to actually delete
    /// anything.
    Uninstall {
        /// Actually perform the removal instead of just printing what
        /// would be removed.
        #[arg(long)]
        yes: bool,

        /// Override both the config and data directories. Hidden:
        /// test-only, so the e2e suite can point removal at a tempdir
        /// instead of the real platform config/data dirs.
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

/// Where this run reads and writes secrets (API keys, the history key).
///
/// Normally that is the OS keychain, where the desktop app moves saved keys
/// out of `config.toml`. An explicit `--config-dir` means a self-contained
/// config (the test suite, a portable setup): secrets then come only from
/// that directory's plaintext `[api_keys]`, and nothing reads, writes or
/// deletes a real keychain entry.
fn cli_keystore(config_dir: Option<&Path>) -> Box<dyn Keystore> {
    match config_dir {
        Some(_) => Box::new(MemoryKeystore::non_persistent()),
        None => Box::new(OsKeystore::new()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = match cli.config_dir.as_deref() {
        Some(dir) => whspr_config::load_from(Some(dir)),
        None => load_config(),
    };
    let keystore = cli_keystore(cli.config_dir.as_deref());

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
            shorten,
        }) => {
            transcribe_cmd::run(
                &config,
                keystore.as_ref(),
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
                shorten,
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
                keystore.as_ref(),
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

        Some(Command::Stats {
            csv,
            clear,
            by_backend,
            data_dir,
        }) => {
            stats_cmd::run(keystore.as_ref(), data_dir, csv, clear, by_backend).await?;
        }

        Some(Command::Uninstall { yes, data_dir }) => {
            uninstall_cmd::run(&config, keystore.as_ref(), data_dir, yes).await?;
        }

        None => {
            anyhow::bail!(
                "no subcommand given; try `whspr transcribe <FILE>` or `whspr --version`"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_config_dir_never_uses_the_os_keychain() {
        let keystore = cli_keystore(Some(Path::new("/tmp/whspr-portable")));
        assert!(!keystore.is_persistent());
        assert_eq!(keystore.get("api-key:openai").unwrap(), None);
    }
}
