//! Model selection for whspr-refine's LLM-backed cleanup backends
//! (`OpenAiRefiner`, `AnthropicRefiner`, `LlamaLocal`), plus free-form
//! instructions layered onto the shared cleanup prompt. `RefineChoice` (in
//! `lib.rs`) picks *which* backend runs; `RefineSettings` picks *which
//! model* (or, for `LlamaLocal`, *which file*) that backend uses.
//!
//! Defined here rather than alongside `Config`'s other settings structs in
//! `lib.rs` (like `SpeakerSettings`) so that crate's file stays under this
//! project's 600-line-per-file guideline (AA-06) -- same reasoning
//! `normalize.rs`/`language.rs` already follow. Named `refine_settings`
//! (table `[refine_settings]`), not `refine`, because `Config::refine`
//! (`RefineChoice`) already owns that field name -- mirrors the existing
//! `language`/`language_settings` split.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Model IDs/paths and free-form instructions for the LLM refiner backends,
/// read from the config file's `[refine_settings]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct RefineSettings {
    /// OpenAI chat/completions model id used by `OpenAiRefiner`.
    pub openai_model: String,
    /// Anthropic Messages API model id used by `AnthropicRefiner`.
    pub anthropic_model: String,
    /// Path to a GGUF model file for `LlamaLocal`. `None` means not
    /// configured yet -- `build_refiner` fails with an honest error until
    /// the user sets this (mirrors `WhisperConfig::model_path`).
    #[serde(default)]
    pub llama_model_path: Option<PathBuf>,
    /// Extra cleanup instructions layered onto the LLM refiners' shared
    /// default guidance (see `whspr_refine::effective_instructions`), e.g.
    /// house style or domain vocabulary. `None` means just the defaults.
    #[serde(default)]
    pub instructions: Option<String>,
}

impl Default for RefineSettings {
    fn default() -> Self {
        Self {
            openai_model: "gpt-4o-mini".to_string(),
            anthropic_model: "claude-3-5-sonnet-20241022".to_string(),
            llama_model_path: None,
            instructions: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_from, Config};

    #[test]
    fn refine_settings_defaults_match_previous_hardcoded_values() {
        assert_eq!(
            RefineSettings::default(),
            RefineSettings {
                openai_model: "gpt-4o-mini".to_string(),
                anthropic_model: "claude-3-5-sonnet-20241022".to_string(),
                llama_model_path: None,
                instructions: None,
            }
        );
    }

    #[test]
    fn refine_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.refine_settings.openai_model = "gpt-4o".to_string();
        cfg.refine_settings.anthropic_model = "claude-3-opus-20240229".to_string();
        cfg.refine_settings.llama_model_path = Some(PathBuf::from("/models/cleanup.gguf"));
        cfg.refine_settings.instructions = Some("Always sign off with my name".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.refine_settings, cfg.refine_settings);
    }

    #[test]
    fn load_from_toml_file_sets_refine_settings() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[refine_settings]").expect("failed to write refine_settings header");
        writeln!(file, "openai-model = \"gpt-4o\"").expect("failed to write openai-model");
        writeln!(file, "llama-model-path = \"/models/cleanup.gguf\"")
            .expect("failed to write llama-model-path");
        writeln!(file, "instructions = \"Keep it terse\"").expect("failed to write instructions");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.refine_settings.openai_model, "gpt-4o");
        assert_eq!(
            cfg.refine_settings.anthropic_model,
            "claude-3-5-sonnet-20241022" // not set in the file - stays default
        );
        assert_eq!(
            cfg.refine_settings.llama_model_path,
            Some(PathBuf::from("/models/cleanup.gguf"))
        );
        assert_eq!(
            cfg.refine_settings.instructions,
            Some("Keep it terse".to_string())
        );
    }

    #[test]
    fn load_from_partial_refine_settings_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[refine_settings]").expect("failed to write refine_settings header");
        writeln!(file, "anthropic-model = \"claude-3-opus-20240229\"")
            .expect("failed to write anthropic-model");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.refine_settings.openai_model, "gpt-4o-mini"); // default
        assert_eq!(
            cfg.refine_settings.anthropic_model,
            "claude-3-opus-20240229"
        );
        assert_eq!(cfg.refine_settings.llama_model_path, None); // default
    }
}
