//! Language settings (B-09, I-03): automatic language detection and fixed
//! language selection for ASR. Defined in its own file to keep `lib.rs`
//! under this project's 600-line-per-file guideline (AA-06).

use serde::{Deserialize, Serialize};

/// Settings for language recognition and selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LanguageSettings {
    /// Whether to auto-detect the language per utterance (true) or use
    /// `fixed_language` if set. Default true (auto-switch recognition
    /// language per utterance).
    pub language_switch: bool,
    /// Fixed language code (e.g., "en", "es", "fr") when `language_switch`
    /// is false. None means no fixed language override is set.
    pub fixed_language: Option<String>,
}

impl Default for LanguageSettings {
    fn default() -> Self {
        Self {
            language_switch: true,
            fixed_language: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{load_from, Config};

    #[test]
    fn language_settings_defaults_to_auto_switch() {
        assert_eq!(
            LanguageSettings::default(),
            LanguageSettings {
                language_switch: true,
                fixed_language: None,
            }
        );
    }

    #[test]
    fn language_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.language_settings.language_switch = false;
        cfg.language_settings.fixed_language = Some("es".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.language_settings, cfg.language_settings);
    }

    #[test]
    fn load_from_toml_file_sets_language_settings() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[language-settings]").expect("failed to write language-settings header");
        writeln!(file, "language-switch = false").expect("failed to write language-switch");
        writeln!(file, "fixed-language = \"es\"").expect("failed to write fixed-language");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.language_settings.language_switch);
        assert_eq!(
            cfg.language_settings.fixed_language,
            Some("es".to_string())
        );
    }

    #[test]
    fn language_settings_partial_toml_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[language-settings]").expect("failed to write language-settings header");
        writeln!(file, "language-switch = false").expect("failed to write language-switch");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.language_settings.language_switch);
        assert_eq!(cfg.language_settings.fixed_language, None); // not set - stays default
    }
}
