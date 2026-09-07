//! HuggingFace Models-tab glue: resolve the models directory, scan installed
//! models, and the async login/download tasks the Hub drives via
//! `Task::perform`. Mirrors `crate::speakers`'s "data-dir helper + background
//! task" shape, but for `whspr-hf` instead of diarization.

use std::path::PathBuf;

use whspr_config::Config;
use whspr_hf::{HfIdentity, InstalledModel, OauthConfig};

/// The platform-default models directory, `<data_dir>/whspr/models`. Used
/// when the user hasn't set `[huggingface].models_dir`. Mirrors
/// `crate::speakers::speaker_db_path`'s use of the app data dir.
pub fn default_models_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "whspr")?;
    Some(dirs.data_dir().join("models"))
}

/// The effective models directory: the configured `[huggingface].models_dir`
/// (or the `WHISPER_MODELS_DIR` env var, via `whspr_hf::resolve_models_dir`),
/// else the platform default.
pub fn models_dir(config: &Config) -> Option<PathBuf> {
    whspr_hf::resolve_models_dir(config.huggingface.models_dir.clone()).or_else(default_models_dir)
}

/// Scans the effective models directory for installed whisper models. Empty
/// if the directory can't be determined or doesn't exist yet.
pub fn scan_installed(config: &Config) -> Vec<InstalledModel> {
    match models_dir(config) {
        Some(dir) => whspr_hf::installed(&dir),
        None => Vec::new(),
    }
}

/// Runs the HuggingFace browser OAuth login, returning `(username, token)`.
/// `client_id` is `[huggingface].oauth_client_id`. Errors are stringified for
/// the Hub's status line.
pub async fn run_login(client_id: String) -> Result<(String, String), String> {
    let HfIdentity { username, token } =
        whspr_hf::run_login(OauthConfig::new(client_id), whspr_hf::DEFAULT_LOGIN_TIMEOUT)
            .await
            .map_err(|e| e.to_string())?;
    Ok((username, token))
}

/// Downloads the curated model `model_id` into `dir`, using `token` (from a
/// completed login or a saved config token) as the bearer if present. Returns
/// the flat on-disk path the model landed at.
pub async fn run_download(
    model_id: &'static str,
    token: Option<String>,
    dir: PathBuf,
) -> Result<PathBuf, String> {
    let model =
        whspr_hf::model_by_id(model_id).ok_or_else(|| format!("unknown model id: {model_id}"))?;
    whspr_hf::download(model, token, &dir, None)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_dir_prefers_configured_over_default() {
        let mut config = Config::default();
        config.huggingface.models_dir = Some(PathBuf::from("/explicit/models"));
        assert_eq!(models_dir(&config), Some(PathBuf::from("/explicit/models")));
    }

    #[test]
    fn scan_installed_of_a_configured_empty_dir_is_empty() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let mut config = Config::default();
        config.huggingface.models_dir = Some(dir.path().to_path_buf());
        assert!(scan_installed(&config).is_empty());
    }

    #[tokio::test]
    async fn run_download_rejects_an_unknown_model_id() {
        let err = run_download("not-a-model", None, PathBuf::from("/tmp"))
            .await
            .expect_err("unknown id should error before any network call");
        assert!(err.contains("unknown model id"), "got: {err}");
    }
}
