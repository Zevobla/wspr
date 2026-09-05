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
    /// Validate the arguments: if using whisper-local, model must be provided.
    pub fn validate(&self) -> Result<(), String> {
        if self.asr == "whisper-local" && self.model.is_none() {
            return Err(
                "error: --asr whisper-local requires --model <path> to be specified".to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_validate_whisper_local_without_model() {
        let args = Args {
            stand_set: PathBuf::from("/tmp/stand"),
            asr: "whisper-local".to_string(),
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
