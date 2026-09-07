//! HuggingFace model-client settings (see the `whspr-hf` crate and the Hub's
//! Models tab): the saved OAuth access token, where downloaded models live,
//! and the OAuth app's client id. In its own file, like the other settings
//! modules, to keep `lib.rs` under this project's 600-line-per-file guideline
//! (AA-06).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Settings for the in-app HuggingFace client, read from the config file's
/// `[huggingface]` table. Every field is optional so an untouched install
/// (no HF account, no downloads) round-trips cleanly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HuggingFaceSettings {
    /// The OAuth access token from a completed browser sign-in (or a token
    /// the user pasted in). Used as the bearer token for downloads.
    ///
    /// Stored in plaintext, mirroring the existing `[api_keys]` precedent
    /// (see `Config::api_keys`). Moving it into the OS keystore (criterion
    /// P-06) is a planned follow-up, not done here. Never read from an
    /// environment variable.
    pub token: Option<String>,
    /// Directory downloaded model files are placed in (a flat directory of
    /// `ggml-*.bin` files; see `whspr_hf::installed`). `None` means the GUI
    /// falls back to a platform default under the app data dir.
    pub models_dir: Option<PathBuf>,
    /// The OAuth app's client id, created once by registering a Connected App
    /// at <https://huggingface.co/settings/applications> (see the `whspr-hf`
    /// crate docs). `None` until the user configures it; sign-in is disabled
    /// until then.
    pub oauth_client_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Config`/`load_from` live in the crate root, not this module -- same
    // pattern the other settings modules' tests follow.
    use crate::{load_from, Config};

    #[test]
    fn huggingface_settings_default_is_all_none() {
        assert_eq!(
            Config::default().huggingface,
            HuggingFaceSettings::default()
        );
        assert!(Config::default().huggingface.token.is_none());
        assert!(Config::default().huggingface.models_dir.is_none());
        assert!(Config::default().huggingface.oauth_client_id.is_none());
    }

    #[test]
    fn huggingface_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.huggingface.token = Some("hf_testtoken".to_string());
        cfg.huggingface.models_dir = Some(PathBuf::from("/models/whisper"));
        cfg.huggingface.oauth_client_id = Some("client-abc123".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.huggingface, cfg.huggingface);
    }

    #[test]
    fn load_from_toml_file_sets_huggingface_fields() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[huggingface]").expect("failed to write header");
        writeln!(file, "oauth-client-id = \"client-abc123\"").expect("failed to write client id");
        writeln!(file, "models-dir = \"/models/whisper\"").expect("failed to write models dir");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(
            cfg.huggingface.oauth_client_id,
            Some("client-abc123".to_string())
        );
        assert_eq!(
            cfg.huggingface.models_dir,
            Some(PathBuf::from("/models/whisper"))
        );
        // A field absent from the file stays `None`.
        assert!(cfg.huggingface.token.is_none());
    }
}
