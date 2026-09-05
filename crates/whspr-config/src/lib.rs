//! App configuration: backend selection and on-disk persistence.
//!
//! Config comes from exactly two sources, merged in priority order:
//! 1. Default config (compiled in, reproducible via the Nix build)
//! 2. The user-editable TOML file in the platform config dir (e.g.
//!    `~/.config/whspr/config.toml` on Linux), overlaid on top of the
//!    defaults.
//!
//! Deliberately *not* a source: environment variables. There is no "local
//! variable" override mechanism — the only way to change a setting is to
//! edit the config file. Don't add `std::env::var` reads here.

use std::collections::BTreeMap;
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
    /// API keys for cloud backends, keyed by backend id (e.g. "openai",
    /// matching `AsrBackend::id()` / `TextRefiner::id()`), read from the
    /// config file's `[api_keys]` table.
    ///
    /// Stored in plaintext in the config file for now. Moving these into
    /// the OS keystore (criterion P-06) is planned for a later privacy
    /// wave and is *not* implemented here — this field is the honest
    /// interim placeholder. Never read from environment variables.
    #[serde(default)]
    pub api_keys: BTreeMap<String, String>,
}

/// Loads the effective config from the platform config directory.
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

    // Overlay the TOML file if it exists. This is the *only* override
    // mechanism whspr has — no environment variables, no other source.
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

    config
}

/// Looks up an API key for a cloud backend by id (e.g. "openai") from the
/// given config's `api_keys` table. Reads only `config`; never an
/// environment variable. See `Config::api_keys` for the plaintext-for-now
/// caveat.
pub fn api_key_for(config: &Config, choice_id: &str) -> Option<String> {
    config.api_keys.get(choice_id).cloned()
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
    fn load_from_ignores_environment_variables() {
        // Regression test: env vars must never override the config file or
        // defaults, even for names the old design specifically read.
        std::env::set_var("WHSPR_ASR", "deepgram");
        std::env::set_var("WHSPR_REFINE", "open-ai");
        std::env::set_var("WHSPR_LANGUAGE", "fr");

        let cfg = load_from(None);

        std::env::remove_var("WHSPR_ASR");
        std::env::remove_var("WHSPR_REFINE");
        std::env::remove_var("WHSPR_LANGUAGE");

        assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
        assert_eq!(cfg.refine, RefineChoice::Noop);
        assert_eq!(cfg.language, None);
    }

    #[test]
    fn api_key_for_reads_from_config_file() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[api_keys]").expect("failed to write api_keys header");
        writeln!(file, "openai = \"sk-test-123\"").expect("failed to write openai key");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(api_key_for(&cfg, "openai"), Some("sk-test-123".to_string()));
        assert_eq!(api_key_for(&cfg, "anthropic"), None);
    }

    #[test]
    fn api_key_for_ignores_environment_variables() {
        std::env::set_var("WHSPR_OPENAI_API_KEY", "should-be-ignored");
        let key = api_key_for(&Config::default(), "openai");
        std::env::remove_var("WHSPR_OPENAI_API_KEY");
        assert_eq!(key, None);
    }
}
