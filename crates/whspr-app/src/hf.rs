//! HuggingFace Models-tab glue: resolve the model directories, scan them for
//! installed models, the async login/download/delete tasks the Hub drives via
//! `Task::perform`, and the `update` handler for the Models-tab messages.
//! Mirrors `crate::speakers`'s "data-dir helper + background task" shape, but
//! for `whspr-hf` instead of diarization. The `update` handler lives here
//! (rather than in `crate::app`) to keep app.rs under the 600-line cap (AA-06).

use std::path::PathBuf;

use iced::Task;
use tokio::sync::mpsc::UnboundedSender;
use whspr_config::Config;
use whspr_hf::{DownloadProgress, HfIdentity, OauthConfig, ScanResult};

use crate::state::{Message, State};

/// GUI state for the refiner section's live HuggingFace GGUF search: the query
/// text, the latest repo hits, which repo (if any) is expanded and its `.gguf`
/// file listing, plus busy/error/searched flags. Kept here (with the rest of
/// the Models-tab glue) rather than in `state.rs` so that file stays under the
/// AA-06 line cap. A search download reuses the existing LLM download + rescan
/// path (see the `LlmSearchDownload` arm in [`update`]), so a searched model
/// appears in the refiner selector exactly like a curated one.
#[derive(Debug, Default)]
pub struct LlmSearchState {
    /// Live contents of the search text input.
    pub query: String,
    /// The most recent search's repo hits (empty before any search).
    pub results: Vec<whspr_hf::GgufRepoHit>,
    /// The repo whose `.gguf` file list is currently expanded, if any.
    pub selected_repo: Option<String>,
    /// The expanded repo's `.gguf` files (path + size), once fetched.
    pub files: Vec<whspr_hf::GgufFile>,
    /// True while a search or file-listing request is in flight.
    pub busy: bool,
    /// The last search/list error to surface to the user, if any.
    pub error: Option<String>,
    /// True once at least one search has completed, so the view can tell an
    /// empty result set ("no results") apart from the initial blank state.
    pub searched: bool,
}

/// Handles the Models-tab (HuggingFace) messages, mutating `state` and
/// returning `Ok(task)`. Any other message is handed straight back as
/// `Err(message)` so `crate::app::update`'s catch-all can forward it to the
/// Settings handler -- keeping app.rs off a second exhaustive match of every
/// Settings message, and under the AA-06 line cap.
pub(crate) fn update(state: &mut State, message: Message) -> Result<Task<Message>, Message> {
    let task = match message {
        Message::HfSignIn => {
            // Use the user's own OAuth app if they set one, otherwise whspr's
            // built-in public client id so browser sign-in works with no setup.
            let client_id = state
                .config
                .huggingface
                .oauth_client_id
                .clone()
                .filter(|id| !id.trim().is_empty())
                .unwrap_or_else(|| whspr_hf::oauth::BUILTIN_CLIENT_ID.to_string());
            state.hf_busy = true;
            state.hf_status = Some("Opening your browser to sign in...".to_string());
            Task::perform(run_login(client_id), Message::HfSignedIn)
        }
        Message::HfTokenInput(token) => {
            state.hf_token_input = token;
            Task::none()
        }
        Message::HfTokenSubmit => {
            // Trim once, off the credential the field holds; if it's blank the
            // submit is a no-op (the button is disabled in that case anyway).
            let token = state.hf_token_input.trim().to_string();
            if token.is_empty() {
                Task::none()
            } else {
                state.hf_busy = true;
                // Clear the field the moment we take the token -- it lives on
                // only inside the validation task now, never re-rendered.
                state.hf_token_input.clear();
                state.hf_status = Some("Checking your token...".to_string());
                Task::perform(run_token_login(token), Message::HfSignedIn)
            }
        }
        Message::HfSignedIn(Ok((username, token))) => {
            state.hf_busy = false;
            state.config.huggingface.token = Some(token);
            state.hf_username = Some(username.clone());
            state.hf_status = Some(format!("Signed in as {username}."));
            crate::app::persist_config(state);
            // A fresh token may unlock gated models -- rescan.
            state.hf_models = scan(&state.config);
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
        Message::HfDownloadModel(model_id) => match start_download(state, model_id) {
            Some(dir) => {
                let token = state.config.huggingface.token.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                Task::batch([
                    crate::hf_progress::progress_task(rx),
                    Task::perform(
                        run_download(model_id, token, dir, Some(tx)),
                        Message::HfModelDownloaded,
                    ),
                ])
            }
            None => Task::none(),
        },
        Message::HfDownloadLlm(model_id) => match start_download(state, model_id) {
            Some(dir) => {
                let token = state.config.huggingface.token.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                Task::batch([
                    crate::hf_progress::progress_task(rx),
                    Task::perform(
                        run_download_llm(model_id, token, dir, Some(tx)),
                        Message::HfLlmDownloaded,
                    ),
                ])
            }
            None => Task::none(),
        },
        Message::HfDownloadProgress { downloaded, total } => {
            if let Some(active) = state.active_download.as_mut() {
                active.update(downloaded, total);
            }
            Task::none()
        }
        Message::HfModelDownloaded(result) => downloaded(state, result, "the ASR list"),
        Message::HfLlmDownloaded(result) => downloaded(state, result, "the Refiner list"),
        Message::HfAsrSelected(option) => {
            crate::model_menu::apply_asr(&mut state.config, &option);
            state.hf_status = Some(format!("Now transcribing with {option}."));
            crate::app::persist_config(state);
            Task::none()
        }
        Message::HfRefineSelected(option) => {
            crate::model_menu::apply_refine(&mut state.config, &option);
            state.hf_status = Some(format!("Refiner set to {option}."));
            crate::app::persist_config(state);
            Task::none()
        }
        Message::HfDeleteModel(path) => {
            state.hf_busy = true;
            state.hf_status = Some(format!("Deleting {}...", model_file_name(&path)));
            Task::perform(run_delete(path), Message::HfModelDeleted)
        }
        Message::HfModelDeleted(Ok(path)) => {
            state.hf_busy = false;
            state.hf_status = Some(format!("Deleted {}.", model_file_name(&path)));
            state.hf_models = scan(&state.config);
            Task::none()
        }
        Message::HfModelDeleted(Err(error)) => {
            state.hf_busy = false;
            state.hf_status = Some(format!("Delete failed: {error}"));
            Task::none()
        }
        Message::LlmSearchInput(query) => {
            state.llm_search.query = query;
            Task::none()
        }
        Message::LlmSearchSubmit => {
            let query = state.llm_search.query.trim().to_string();
            if query.is_empty() {
                Task::none()
            } else {
                state.llm_search.busy = true;
                state.llm_search.error = None;
                state.llm_search.selected_repo = None;
                state.llm_search.files.clear();
                let token = state.config.huggingface.token.clone();
                Task::perform(run_search_llm(query, token), Message::LlmSearchResults)
            }
        }
        Message::LlmSearchResults(Ok(results)) => {
            state.llm_search.busy = false;
            state.llm_search.searched = true;
            state.llm_search.results = results;
            Task::none()
        }
        Message::LlmSearchResults(Err(error)) => {
            state.llm_search.busy = false;
            state.llm_search.searched = true;
            state.llm_search.results.clear();
            state.llm_search.error = Some(format!("Search failed: {error}"));
            Task::none()
        }
        Message::LlmSearchSelectRepo(repo) => {
            // Toggle: clicking the already-open repo collapses its file list.
            if state.llm_search.selected_repo.as_deref() == Some(repo.as_str()) {
                state.llm_search.selected_repo = None;
                state.llm_search.files.clear();
                Task::none()
            } else {
                state.llm_search.selected_repo = Some(repo.clone());
                state.llm_search.files.clear();
                state.llm_search.busy = true;
                state.llm_search.error = None;
                let token = state.config.huggingface.token.clone();
                Task::perform(run_list_gguf_files(repo, token), Message::LlmSearchFiles)
            }
        }
        Message::LlmSearchFiles(Ok(files)) => {
            state.llm_search.busy = false;
            state.llm_search.files = files;
            Task::none()
        }
        Message::LlmSearchFiles(Err(error)) => {
            state.llm_search.busy = false;
            state.llm_search.error = Some(format!("Could not list files: {error}"));
            Task::none()
        }
        Message::LlmSearchDownload(repo, filename) => match start_download(state, &filename) {
            Some(dir) => {
                let token = state.config.huggingface.token.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                Task::batch([
                    crate::hf_progress::progress_task(rx),
                    Task::perform(
                        run_download_gguf(repo, filename, token, dir, Some(tx)),
                        Message::HfLlmDownloaded,
                    ),
                ])
            }
            None => Task::none(),
        },
        Message::HfAddModelDir => Task::perform(pick_model_dir(), Message::HfModelDirPicked),
        Message::HfModelDirPicked(None) => Task::none(),
        Message::HfModelDirPicked(Some(dir)) => {
            if !state.config.huggingface.model_dirs.contains(&dir) {
                state.hf_status = Some(format!("Scanning {}...", dir.display()));
                state.config.huggingface.model_dirs.push(dir);
                crate::app::persist_config(state);
                state.hf_models = scan(&state.config);
            }
            Task::none()
        }
        Message::HfRemoveModelDir(dir) => {
            state.config.huggingface.model_dirs.retain(|d| d != &dir);
            crate::app::persist_config(state);
            state.hf_models = scan(&state.config);
            Task::none()
        }
        other => return Err(other),
    };
    Ok(task)
}

/// Shared "start a download" bookkeeping for both whisper and LLM: resolves
/// the target [`download_dir`], and on success flips `hf_busy`, arms the live
/// [`ActiveDownload`](crate::hf_progress::ActiveDownload) progress indicator
/// (which replaces the old static "Downloading..." status line -- see
/// `crate::hf_progress`), and returns the dir to download into. `None` (with
/// an error status set) when no models directory can be determined.
fn start_download(state: &mut State, model_id: &str) -> Option<PathBuf> {
    match download_dir(&state.config) {
        Some(dir) => {
            state.hf_busy = true;
            state.hf_status = None;
            state.active_download = Some(crate::hf_progress::ActiveDownload::new(
                model_id.to_string(),
            ));
            Some(dir)
        }
        None => {
            state.hf_status =
                Some("Could not determine a models directory to download into.".to_string());
            None
        }
    }
}

/// Shared "a download finished" arm: clears `hf_busy`, reports success (naming
/// which selector to pick the model in) or the error, and rescans on success.
fn downloaded(state: &mut State, result: Result<PathBuf, String>, selector: &str) -> Task<Message> {
    state.hf_busy = false;
    // The download is over -- tear down the live progress indicator whether it
    // succeeded or failed, so the bar never lingers past completion.
    state.active_download = None;
    match result {
        Ok(path) => {
            state.hf_status = Some(format!(
                "Downloaded {}. Select it in {selector} to use it.",
                model_file_name(&path)
            ));
            state.hf_models = scan(&state.config);
        }
        Err(error) => state.hf_status = Some(format!("Download failed: {error}")),
    }
    Task::none()
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

/// The directory a fresh download is placed in: the first user-added model
/// directory if any, else the default [`models_dir`]. Both are always part of
/// [`effective_model_dirs`], so a downloaded model always appears in a scan.
fn download_dir(config: &Config) -> Option<PathBuf> {
    config
        .huggingface
        .model_dirs
        .first()
        .cloned()
        .or_else(|| models_dir(config))
}

/// Every directory the Models tab scans: the user-managed
/// `[huggingface].model_dirs` plus the default [`models_dir`] (deduplicated),
/// so the default download location is always included even when the user has
/// added extra directories.
pub fn effective_model_dirs(config: &Config) -> Vec<PathBuf> {
    let mut dirs = config.huggingface.model_dirs.clone();
    if let Some(default) = models_dir(config) {
        if !dirs.contains(&default) {
            dirs.push(default);
        }
    }
    dirs
}

/// Scans every [`effective_model_dirs`] entry for installed models, split into
/// ASR + LLM buckets. Empty if no directory exists yet.
pub fn scan(config: &Config) -> ScanResult {
    whspr_hf::scan(&effective_model_dirs(config))
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

/// Validates a pasted HuggingFace access `token` by resolving its account
/// username via `whspr_hf::oauth::whoami`, returning `(username, token)` on
/// success so the existing `Message::HfSignedIn(Ok(..))` handler persists the
/// token exactly the way the OAuth flow does -- no separate persistence path.
/// The token is treated as a credential and never logged. Errors are
/// stringified for the Hub's status line.
pub async fn run_token_login(token: String) -> Result<(String, String), String> {
    let username = whspr_hf::oauth::whoami(&token)
        .await
        .map_err(|e| e.to_string())?;
    Ok((username, token))
}

/// Downloads the curated whisper model `model_id` into `dir`, using `token`
/// (from a completed login or a saved config token) as the bearer if present.
/// Returns the flat on-disk path the model landed at.
pub async fn run_download(
    model_id: &'static str,
    token: Option<String>,
    dir: PathBuf,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf, String> {
    let model =
        whspr_hf::model_by_id(model_id).ok_or_else(|| format!("unknown model id: {model_id}"))?;
    whspr_hf::download(model, token, &dir, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Downloads the curated GGUF refiner LLM `model_id` into `dir`. Same
/// token/return semantics as [`run_download`].
pub async fn run_download_llm(
    model_id: &'static str,
    token: Option<String>,
    dir: PathBuf,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf, String> {
    let model = whspr_hf::llm_model_by_id(model_id)
        .ok_or_else(|| format!("unknown llm model id: {model_id}"))?;
    whspr_hf::download_llm(model, token, &dir, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Searches HuggingFace for GGUF refiner repos matching `query`, using the
/// saved `token` (if any) as the bearer. Errors are stringified for the search
/// section's error line.
pub async fn run_search_llm(
    query: String,
    token: Option<String>,
) -> Result<Vec<whspr_hf::GgufRepoHit>, String> {
    whspr_hf::search_gguf(&query, token.as_deref(), whspr_hf::DEFAULT_SEARCH_LIMIT)
        .await
        .map_err(|e| e.to_string())
}

/// Lists the `.gguf` files in `repo` (with sizes, including subfolders), using
/// the saved `token` if present. Errors are stringified for the search
/// section's error line.
pub async fn run_list_gguf_files(
    repo: String,
    token: Option<String>,
) -> Result<Vec<whspr_hf::GgufFile>, String> {
    whspr_hf::list_gguf_files(&repo, token.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Downloads a searched GGUF `filename` from `repo` into `dir`, reusing the
/// same flat-file download path the curated LLMs use so the result flows
/// through the existing rescan (`HfLlmDownloaded`). Same token/return semantics
/// as [`run_download_llm`].
pub async fn run_download_gguf(
    repo: String,
    filename: String,
    token: Option<String>,
    dir: PathBuf,
    progress: Option<UnboundedSender<DownloadProgress>>,
) -> Result<PathBuf, String> {
    whspr_hf::download_gguf(&repo, &filename, token, &dir, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Deletes the model file at `path`, returning the path on success so the
/// caller can name it in a status message before rescanning.
pub async fn run_delete(path: PathBuf) -> Result<PathBuf, String> {
    whspr_hf::delete(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Opens a native folder picker for "Add directory". Resolves to `None` if the
/// user cancels.
pub async fn pick_model_dir() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
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
    fn effective_model_dirs_includes_the_default_and_user_dirs() {
        let mut config = Config::default();
        config.huggingface.models_dir = Some(PathBuf::from("/default/models"));
        config.huggingface.model_dirs = vec![PathBuf::from("/user/ggufs")];

        let dirs = effective_model_dirs(&config);
        assert!(dirs.contains(&PathBuf::from("/user/ggufs")));
        assert!(dirs.contains(&PathBuf::from("/default/models")));
    }

    #[test]
    fn effective_model_dirs_does_not_duplicate_the_default() {
        let mut config = Config::default();
        config.huggingface.models_dir = Some(PathBuf::from("/models"));
        config.huggingface.model_dirs = vec![PathBuf::from("/models")];

        let dirs = effective_model_dirs(&config);
        assert_eq!(
            dirs.iter()
                .filter(|d| *d == &PathBuf::from("/models"))
                .count(),
            1
        );
    }

    #[test]
    fn download_dir_prefers_first_user_dir() {
        let mut config = Config::default();
        config.huggingface.models_dir = Some(PathBuf::from("/default"));
        config.huggingface.model_dirs = vec![PathBuf::from("/first"), PathBuf::from("/second")];
        assert_eq!(download_dir(&config), Some(PathBuf::from("/first")));
    }

    #[test]
    fn scan_of_a_configured_empty_dir_is_empty() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let mut config = Config::default();
        config.huggingface.models_dir = Some(dir.path().to_path_buf());
        let result = scan(&config);
        assert!(result.asr.is_empty() && result.llm.is_empty());
    }

    #[tokio::test]
    async fn run_download_rejects_an_unknown_model_id() {
        let err = run_download("not-a-model", None, PathBuf::from("/tmp"), None)
            .await
            .expect_err("unknown id should error before any network call");
        assert!(err.contains("unknown model id"), "got: {err}");
    }

    #[tokio::test]
    async fn run_download_llm_rejects_an_unknown_model_id() {
        let err = run_download_llm("not-a-model", None, PathBuf::from("/tmp"), None)
            .await
            .expect_err("unknown id should error before any network call");
        assert!(err.contains("unknown llm model id"), "got: {err}");
    }
}
