//! App configuration: backend selection and on-disk persistence.
//! Loads configuration by merging (in priority order):
//! 1. Default config (built-in fallback)
//! 2. Platform config file (e.g. ~/.config/whspr/config.toml on Linux)
//! 3. Environment variable overrides (WHSPR_ASR, WHSPR_LANGUAGE, etc.)

use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AsrChoice {
    #[default]
    WhisperLocal,
    OpenAi,
    Deepgram,
}

impl FromStr for AsrChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "whisper-local" | "whisper_local" | "whisperloca" => Ok(AsrChoice::WhisperLocal),
            "openai" | "open-ai" | "open_ai" => Ok(AsrChoice::OpenAi),
            "deepgram" => Ok(AsrChoice::Deepgram),
            _ => Err(format!("unknown ASR choice: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefineChoice {
    #[default]
    Noop,
    OpenAi,
    Anthropic,
    LlamaLocal,
}

impl FromStr for RefineChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "noop" => Ok(RefineChoice::Noop),
            "openai" | "open-ai" | "open_ai" => Ok(RefineChoice::OpenAi),
            "anthropic" => Ok(RefineChoice::Anthropic),
            "llama-local" | "llama_local" | "llamalocal" => Ok(RefineChoice::LlamaLocal),
            _ => Err(format!("unknown refine choice: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub asr: AsrChoice,
    #[serde(default)]
    pub refine: RefineChoice,
    #[serde(default)]
    pub language: Option<String>,
}

/// Loads the effective config from platform directories and environment.
/// Falls back gracefully to defaults on any error (config loading should never crash).
pub fn load() -> Config {
    if let Some(project_dirs) = directories::ProjectDirs::from("", "", "whspr") {
        let config_dir = project_dirs.config_dir();
        load_from(Some(config_dir))
    } else {
        // If we can't determine platform config dir, use defaults
        Config::default()
    }
}

/// Loads config from a specified directory. This is the testable version.
/// Falls back to defaults on any error.
pub fn load_from(config_dir: Option<&Path>) -> Config {
    // Start with defaults
    let mut config = Config::default();

    // Load from TOML file if it exists
    if let Some(dir) = config_dir {
        let config_path = dir.join("config.toml");
        if config_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config_path) {
                if let Ok(file_config) = toml::from_str::<Config>(&contents) {
                    config = file_config;
                }
            }
        }
    }

    // Environment variables override file config
    if let Ok(asr_str) = std::env::var("WHSPR_ASR") {
        if let Ok(asr) = asr_str.parse::<AsrChoice>() {
            config.asr = asr;
        }
    }

    if let Ok(refine_str) = std::env::var("WHSPR_REFINE") {
        if let Ok(refine) = refine_str.parse::<RefineChoice>() {
            config.refine = refine;
        }
    }

    if let Ok(language) = std::env::var("WHSPR_LANGUAGE") {
        config.language = Some(language);
    }

    config
}

/// Looks up API keys from environment variables.
/// Checks WHSPR_<CHOICE>_API_KEY, e.g. WHSPR_OPENAI_API_KEY.
pub fn api_key_for(choice_id: &str) -> Option<String> {
    let env_var = format!("WHSPR_{}_API_KEY", choice_id.to_uppercase());
    std::env::var(&env_var).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_returns_defaults() {
        let cfg = load();
        assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
        assert_eq!(cfg.refine, RefineChoice::Noop);
        assert_eq!(cfg.language, None);
    }

    #[test]
    fn load_from_none_returns_defaults() {
        let cfg = load_from(None);
        assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
        assert_eq!(cfg.refine, RefineChoice::Noop);
        assert_eq!(cfg.language, None);
    }

    #[test]
    fn load_from_toml_file_overrides() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "asr = \"open-ai\"").expect("failed to write asr");
        writeln!(file, "refine = \"open-ai\"").expect("failed to write refine");
        writeln!(file, "language = \"es\"").expect("failed to write language");
        drop(file); // ensure file is closed

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.asr, AsrChoice::OpenAi);
        assert_eq!(cfg.refine, RefineChoice::OpenAi);
        assert_eq!(cfg.language, Some("es".to_string()));
    }

    #[test]
    fn load_from_partial_toml_merges_with_defaults() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "asr = \"deepgram\"").expect("failed to write asr");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.asr, AsrChoice::Deepgram);
        assert_eq!(cfg.refine, RefineChoice::Noop); // default
        assert_eq!(cfg.language, None); // default
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
        assert_eq!(cfg.refine, RefineChoice::Noop);
        assert_eq!(cfg.language, None);
    }

    #[test]
    fn api_key_for_looks_up_env_var() {
        std::env::set_var("WHSPR_OPENAI_API_KEY", "test-key-123");
        let key = api_key_for("openai");
        assert_eq!(key, Some("test-key-123".to_string()));
        std::env::remove_var("WHSPR_OPENAI_API_KEY");
    }

    #[test]
    fn api_key_for_returns_none_if_not_set() {
        std::env::remove_var("WHSPR_UNKNOWN_API_KEY");
        let key = api_key_for("unknown");
        assert_eq!(key, None);
    }
}
