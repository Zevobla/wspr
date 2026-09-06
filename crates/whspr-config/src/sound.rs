//! Sound-feedback setting (AG-05): whether whspr plays a short tone when
//! recording starts and stops (see `whspr-app`'s `sound` module for the
//! actual playback). Defined in its own file, like `AutostartSettings` in
//! `autostart.rs`, to keep `lib.rs` under this project's 600-line-per-file
//! guideline (AA-06).

use serde::{Deserialize, Serialize};

/// Whether whspr plays a short audio cue on recording start/stop. On by
/// default -- audible start/stop feedback is standard for dictation
/// tools; a single Hub checkbox turns it off for anyone who doesn't want
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SoundSettings {
    pub enabled: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Config`/`load_from` live in the crate root, not this module -- same
    // pattern `autostart.rs`'s tests already follow for `AutostartSettings`.
    use crate::{load_from, Config};

    #[test]
    fn sound_settings_defaults_to_enabled() {
        assert_eq!(Config::default().sound, SoundSettings { enabled: true });
    }

    #[test]
    fn sound_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.sound.enabled = false;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.sound, cfg.sound);
    }

    #[test]
    fn load_from_toml_file_sets_sound_enabled() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[sound]").expect("failed to write sound header");
        writeln!(file, "enabled = false").expect("failed to write enabled");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.sound.enabled);
    }
}
