use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "whspr-bench")]
#[command(about = "Benchmark ASR accuracy against a test set", long_about = None)]
pub struct Args {
    /// Path to the stand set directory (containing аудио/ and эталоны.json).
    #[arg(long, required = true)]
    pub stand_set: PathBuf,

    /// ASR backend to use: "mock" or "whisper-local".
    #[arg(long, default_value = "mock")]
    pub asr: String,

    /// Path to the model file (required if --asr is "whisper-local").
    #[arg(long)]
    pub model: Option<PathBuf>,

    /// Language code for ASR (default "ru" for Russian).
    #[arg(long, default_value = "ru")]
    pub language: String,

    /// Output results as JSON instead of plain text.
    #[arg(long)]
    pub json: bool,
}

impl Args {
    /// Fails fast on structural argument problems before any work (loading
    /// the stand set, decoding audio, ...) begins.
    ///
    /// Deliberately does NOT check model availability for `--asr
    /// whisper-local` here: a model path can come from `--model` or, inside
    /// the Nix devShell, from the `WHISPER_MODEL_PATH` env var that points
    /// at the project's pinned model (see `WhisperLocal::resolve_model_path`
    /// in `whspr-asr`). That resolution only makes sense at the point the
    /// backend is actually constructed in `main`, not here.
    pub fn validate(&self) -> Result<(), String> {
        match self.asr.as_str() {
            "mock" | "whisper-local" => Ok(()),
            other => Err(format!(
                "unknown ASR backend: {} (supported: mock, whisper-local)",
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_validate_whisper_local_without_model_is_structurally_ok() {
        // No --model is fine at the Args level: WhisperLocal::resolve_model_path
        // may still find one via WHISPER_MODEL_PATH at construction time in
        // main(). Missing-model failure is surfaced there, not here.
        let args = Args {
            stand_set: PathBuf::from("/tmp/stand"),
            asr: "whisper-local".to_string(),
            model: None,
            language: "ru".to_string(),
            json: false,
        };
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_unknown_backend() {
        let args = Args {
            stand_set: PathBuf::from("/tmp/stand"),
            asr: "carrier-pigeon".to_string(),
            model: None,
            language: "ru".to_string(),
            json: false,
        };
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_args_validate_whisper_local_with_model() {
        let args = Args {
            stand_set: PathBuf::from("/tmp/stand"),
            asr: "whisper-local".to_string(),
            model: Some(PathBuf::from("/path/to/model.bin")),
            language: "ru".to_string(),
            json: false,
        };
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_mock() {
        let args = Args {
            stand_set: PathBuf::from("/tmp/stand"),
            asr: "mock".to_string(),
            model: None,
            language: "ru".to_string(),
            json: false,
        };
        assert!(args.validate().is_ok());
    }
}
