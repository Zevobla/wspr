//! HuggingFace Models-tab glue: resolve the models directory, scan installed
//! models, the async login/download tasks the Hub drives via `Task::perform`,
//! and the `update` handler for the six Models-tab messages. Mirrors
//! `crate::speakers`'s "data-dir helper + background task" shape, but for
//! `whspr-hf` instead of diarization. The `update` handler lives here (rather
//! than in `crate::app`) to keep app.rs under the 600-line cap (AA-06).

use std::path::PathBuf;

use iced::Task;
use whspr_config::Config;
use whspr_hf::{HfIdentity, InstalledModel, OauthConfig};

use crate::state::{Message, State};

/// Handles the six Models-tab (HuggingFace) messages, mutating `state` and
/// returning `Ok(task)`. Any other message is handed straight back as
/// `Err(message)` so `crate::app::update`'s catch-all can forward it to the
/// Settings handler -- keeping app.rs off a second exhaustive match of every
/// Settings message, and under the AA-06 line cap.
pub(crate) fn update(state: &mut State, message: Message) -> Result<Task<Message>, Message> {
    let task = match message {
        Message::HfSignIn => match state.config.huggingface.oauth_client_id.clone() {
            Some(client_id) if !client_id.trim().is_empty() => {
                state.hf_busy = true;
                state.hf_status = Some("Opening your browser to sign in...".to_string());
                Task::perform(run_login(client_id), Message::HfSignedIn)
            }
            _ => {
                state.hf_status =
                    Some("Set [huggingface].oauth-client-id in your config first.".to_string());
                Task::none()
            }
        },
        Message::HfSignedIn(Ok((username, token))) => {
            state.hf_busy = false;
            state.config.huggingface.token = Some(token);
            state.hf_username = Some(username.clone());
            state.hf_status = Some(format!("Signed in as {username}."));
            crate::app::persist_config(state);
            // A fresh token may unlock gated models -- rescan.
            state.hf_installed = scan_installed(&state.config);
            Task::none()
        }
        Message::HfSignedIn(Err(error)) => {
            state.hf_busy = false;
            state.hf_status = Some(format!("Sign-in failed: {error}"));
            Task::none()
        }
        Message::HfSignOut => {
            state.config.huggingface.token = None;
            state.hf_username = None;
            state.hf_status = Some("Signed out.".to_string());
            crate::app::persist_config(state);
            Task::none()
        }
        Message::HfDownloadModel(model_id) => match models_dir(&state.config) {
            Some(dir) => {
                state.hf_busy = true;
                state.hf_status = Some(format!("Downloading {model_id}... this can take a while."));
                let token = state.config.huggingface.token.clone();
                Task::perform(
                    run_download(model_id, token, dir),
                    Message::HfModelDownloaded,
                )
            }
            None => {
                state.hf_status =
                    Some("Could not determine a models directory to download into.".to_string());
                Task::none()
            }
        },
        Message::HfModelDownloaded(Ok(path)) => {
            state.hf_busy = false;
            let name = model_file_name(&path);
            state.hf_status = Some(format!(
                "Downloaded {name}. Click \"Use this model\" to apply."
            ));
            state.hf_installed = scan_installed(&state.config);
            Task::none()
        }
        Message::HfModelDownloaded(Err(error)) => {
            state.hf_busy = false;
            state.hf_status = Some(format!("Download failed: {error}"));
            Task::none()
        }
        Message::HfUseModel(path) => {
            let name = model_file_name(&path);
            state.config.whisper.model_path = Some(path);
            state.hf_status = Some(format!("Now dictating with {name}."));
            crate::app::persist_config(state);
            Task::none()
        }
        other => return Err(other),
    };
    Ok(task)
}

/// The file name of a model path for a status message, falling back to the
/// full path if it somehow has no final component.
fn model_file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

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
