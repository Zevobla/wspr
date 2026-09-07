//! In-app HuggingFace client for whspr: sign in with HuggingFace (browser
//! OAuth), browse a curated set of whisper.cpp ASR models with a "fits your
//! machine" badge, download one into a local models directory, and list
//! what's already installed -- so the GUI can point
//! `config.whisper.model_path` at a real file without the user ever editing
//! the config by hand.
//!
//! This is a leaf crate: it depends only on `whspr-core` (for the shared
//! `Result`/`WhsprError`) and the HuggingFace/OS deps (`hf-hub`, `oauth2`,
//! `sysinfo`, `webbrowser`), never on the GUI. It mirrors `whspr-diarize`'s
//! shape: pure, testable logic (the fit heuristic, the model registry, the
//! PKCE/URL builders) with the network + browser edges kept behind thin
//! functions the tests leave alone.
//!
//! # Modules
//!
//! - [`hardware`] -- `sysinfo` RAM probe + the pure red/yellow/green fit
//!   heuristic.
//! - [`models`] -- the curated whisper.cpp GGML (ASR) and instruct-GGUF
//!   (refiner LLM) registries, `hf-hub`-backed `download`/`download_llm`, a
//!   magic-byte `classify`, a unified multi-directory `scan`, and `delete`.
//! - [`oauth`] -- the HuggingFace OAuth PKCE flow (authorize URL, local
//!   redirect catcher, code->token exchange, `whoami`).
//!
//! # One-time HuggingFace OAuth app registration (required)
//!
//! The OAuth flow needs a *client id*, which the user creates once by
//! registering a "Connected App" (OAuth app) on HuggingFace:
//!
//! 1. Visit <https://huggingface.co/settings/applications> and click
//!    **"Create a new OAuth application"**.
//! 2. Set the **Redirect URI** to exactly
//!    `http://localhost:8788/callback` (the default -- see
//!    [`oauth::DEFAULT_REDIRECT_PORT`]; if you change the port in config, the
//!    registered URI must match it).
//! 3. Grant the scopes whspr uses (see [`oauth::DEFAULT_SCOPES`]:
//!    `openid`, `profile`, `read-repos`).
//! 4. Copy the generated **Client ID** into whspr's config under
//!    `[huggingface] oauth-client-id = "..."` (the GUI's Models tab reads it
//!    from `whspr_config::HuggingFaceSettings::oauth_client_id`).
//!
//! There is no client *secret*: whspr is a native public client and uses PKCE
//! instead, so nothing secret is ever stored in the config file.

pub mod hardware;
pub mod models;
pub mod oauth;

pub use hardware::{estimated_footprint, fits, probe, Fit, HardwareSpecs};
pub use models::{
    classify, delete, download, download_llm, human_size, installed, llm_model_by_filename,
    llm_model_by_id, model_by_filename, model_by_id, resolve_models_dir, scan, DownloadProgress,
    InstalledModel, LlmModel, LocalModel, ModelKind, ScanResult, WhisperModel, LLM_MODELS, MODELS,
    REPO,
};
pub use oauth::{
    run_login, AuthSession, HfIdentity, OauthConfig, DEFAULT_LOGIN_TIMEOUT, DEFAULT_REDIRECT_PORT,
};
