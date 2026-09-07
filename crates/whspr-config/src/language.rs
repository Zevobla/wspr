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

/// Resolves the `Option<String>` language hint that should actually reach
/// `Pipeline::with_language`/`AsrOptions.language`, from `language_settings`
/// plus the top-level manual override `language` (what the Hub's language
/// `pick_list` edits).
///
/// - `language_settings.language_switch == true` (the default): `None` --
///   whisper's own per-utterance auto-detect across every language it
///   supports (~99 languages), not limited to any particular pair. Whisper
///   has no "constrain detection to a subset" mode -- the language hint is
///   either a single fixed language or full auto-detect -- so this is the
///   only way to get multilingual dictation working out of the box without
///   the user picking anything.
/// - `language_switch == false`: `language_settings.fixed_language` if set,
///   otherwise the manual override `language` -- so turning auto-switch off
///   without also setting a fixed language just falls back to whatever the
///   pick_list already had selected, rather than silently going back to
///   auto-detect.
///
/// Pure and side-effect-free so every caller that builds a `Pipeline`
/// (live dictation in `whspr-app`'s worker, the file-transcribe path, and
/// eventually the CLI) resolves the same value the same way.
pub fn effective_language(
    language_settings: &LanguageSettings,
    language: &Option<String>,
) -> Option<String> {
    if language_settings.language_switch {
        None
    } else {
        language_settings
            .fixed_language
            .clone()
            .or_else(|| language.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{load_from, Config};

    #[test]
    fn effective_language_is_none_when_auto_switch_is_on() {
        let settings = LanguageSettings {
            language_switch: true,
            fixed_language: Some("es".to_string()),
        };

        // Auto-switch wins even if a fixed language is also set -- full
        // multilingual auto-detect is the default and takes priority.
        assert_eq!(effective_language(&settings, &Some("fr".to_string())), None);
    }

    #[test]
    fn effective_language_uses_fixed_language_when_auto_switch_is_off() {
        let settings = LanguageSettings {
            language_switch: false,
            fixed_language: Some("es".to_string()),
        };

        assert_eq!(
            effective_language(&settings, &Some("fr".to_string())),
            Some("es".to_string())
        );
    }

    #[test]
    fn effective_language_falls_back_to_manual_override_without_fixed_language() {
        let settings = LanguageSettings {
            language_switch: false,
            fixed_language: None,
        };

        assert_eq!(
            effective_language(&settings, &Some("fr".to_string())),
            Some("fr".to_string())
        );
    }

    #[test]
    fn effective_language_is_none_without_auto_switch_fixed_language_or_override() {
        let settings = LanguageSettings {
            language_switch: false,
            fixed_language: None,
        };

        assert_eq!(effective_language(&settings, &None), None);
    }

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
        writeln!(file, "[language_settings]").expect("failed to write language_settings header");
        writeln!(file, "language-switch = false").expect("failed to write language-switch");
        writeln!(file, "fixed-language = \"es\"").expect("failed to write fixed-language");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.language_settings.language_switch);
        assert_eq!(cfg.language_settings.fixed_language, Some("es".to_string()));
    }

    #[test]
    fn language_settings_partial_toml_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[language_settings]").expect("failed to write language_settings header");
        writeln!(file, "language-switch = false").expect("failed to write language-switch");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.language_settings.language_switch);
        assert_eq!(cfg.language_settings.fixed_language, None); // not set - stays default
    }
}
