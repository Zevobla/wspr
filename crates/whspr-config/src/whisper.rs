//! Settings for the local whisper.cpp ASR backend (`WhisperLocal`).
//!
//! Split into its own file rather than sitting inline in `lib.rs` (next to
//! `Config`) so that crate's file stays under this project's 600-line-per-
//! file guideline (AA-06) -- same reasoning `normalize.rs`/`language.rs`/
//! `refine.rs` already follow.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Settings for the local whisper.cpp backend. Config-file-only like
/// `Config::api_keys` (no env var fallback) — see `lib.rs`'s module doc
/// comment.
///
/// `WhisperLocal::new(path)` (in `whspr-asr`) already accepts any path
/// directly, so this field is a convenience for whoever eventually wires
/// config into backend construction (e.g. `whspr-cli`); that wiring is not
/// done here, out of scope for this crate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhisperConfig {
    /// Path to a GGML model file (e.g. `ggml-base.bin`). `None` means no
    /// path has been configured yet.
    #[serde(default)]
    pub model_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // `Config`/`load_from` live in the crate root, not this module, but
    // these exercise `WhisperConfig` through them the same way `lib.rs`'s
    // test module does for `SpeakerSettings`.
    use crate::{load_from, Config};

    #[test]
    fn whisper_model_path_defaults_to_none() {
        let cfg = Config::default();
        assert_eq!(cfg.whisper.model_path, None);
    }

    #[test]
    fn load_from_toml_file_sets_whisper_model_path() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[whisper]").expect("failed to write whisper header");
        writeln!(file, "model_path = \"/models/ggml-base.bin\"")
            .expect("failed to write model_path");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(
            cfg.whisper.model_path,
            Some(PathBuf::from("/models/ggml-base.bin"))
        );
    }

    #[test]
    fn whisper_config_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.whisper.model_path = Some(PathBuf::from("/models/ggml-base.bin"));

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.whisper, cfg.whisper);
    }
}
