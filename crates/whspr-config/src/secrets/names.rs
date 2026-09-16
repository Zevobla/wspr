//! Naming scheme for entries in the [`super::Keystore`]. Every secret
//! whspr stores outside the plaintext config file is addressed by one of
//! these names, so the naming stays centralized instead of each call site
//! inventing its own string.

/// Namespace for the [`super::Keystore`] entry names whspr uses. Not
/// instantiated -- `SecretName::api_key(...)` / `SecretName::HF_TOKEN` /
/// `SecretName::HISTORY_KEY` are the entry points.
pub struct SecretName;

impl SecretName {
    /// The keystore entry name for a cloud backend's API key, e.g.
    /// `SecretName::api_key("openai") == "api-key:openai"`. `backend_id`
    /// matches `AsrBackend::id()` / `TextRefiner::id()` -- the same id the
    /// legacy `[api_keys]` table keys on.
    pub fn api_key(backend_id: &str) -> String {
        format!("api-key:{backend_id}")
    }

    /// The keystore entry name for the HuggingFace OAuth access token
    /// (replaces `HuggingFaceSettings::token`).
    pub const HF_TOKEN: &'static str = "hf-token";

    /// The keystore entry name for the random symmetric key used to
    /// encrypt the history file at rest when `[privacy].history_encryption`
    /// is on (see [`super::history_key`]).
    pub const HISTORY_KEY: &'static str = "history-key";
}
