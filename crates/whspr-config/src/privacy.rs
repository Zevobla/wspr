//! Privacy and security settings (AG-01, AG-03, M-17): microphone privacy
//! mode and encryption preferences. Defined in its own file to keep lib.rs
//! under this project's 600-line-per-file guideline (AA-06).

use serde::{Deserialize, Serialize};

/// Privacy and security settings for the whspr application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PrivacySettings {
    /// Whether the microphone is released/turned off outside of active capture.
    /// Default true — when not actively recording, the mic stays isolated.
    pub mic_privacy: bool,
    /// Whether transcripts stored in history are encrypted at rest.
    /// Default false — plaintext history for now; encryption is a future security wave.
    pub history_encryption: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            mic_privacy: true,
            history_encryption: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_from, Config};

    #[test]
    fn privacy_settings_defaults_mic_privacy_true_encryption_false() {
        assert_eq!(
            PrivacySettings::default(),
            PrivacySettings {
                mic_privacy: true,
                history_encryption: false,
            }
        );
    }

    #[test]
    fn privacy_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.privacy.mic_privacy = false;
        cfg.privacy.history_encryption = true;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.privacy, cfg.privacy);
    }

    #[test]
    fn load_from_toml_file_sets_privacy_settings() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[privacy]").expect("failed to write privacy header");
        writeln!(file, "mic-privacy = false").expect("failed to write mic-privacy");
        writeln!(file, "history-encryption = true").expect("failed to write history-encryption");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.privacy.mic_privacy);
        assert!(cfg.privacy.history_encryption);
    }

    #[test]
    fn privacy_settings_partial_toml_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[privacy]").expect("failed to write privacy header");
        writeln!(file, "mic-privacy = false").expect("failed to write mic-privacy");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.privacy.mic_privacy);
        assert!(!cfg.privacy.history_encryption); // not set in file - stays default
    }
}
