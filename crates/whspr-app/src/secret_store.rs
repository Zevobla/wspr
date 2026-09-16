//! The app's side of OS-keystore secret storage (criterion P-06): API keys
//! and the HuggingFace token live in the macOS Keychain / Windows Credential
//! Manager when that store survives a reboot, and in `config.toml` (as
//! before) when it does not.
//!
//! `whspr_config` owns the storage primitives (`Keystore`, `OsKeystore`,
//! `Config::resolve_api_key`, `Config::migrate_secrets_to_keystore`); this
//! module decides where a secret the user edits goes, and remembers where
//! each one lives so views never query the keychain on a redraw.

use std::sync::Arc;

use whspr_config::{Config, Keystore, MemoryKeystore, OsKeystore, SecretName};

/// A shared keystore handle: `OsKeystore` in the running app, a
/// `MemoryKeystore` in tests. Wrapped so `State` can derive `Debug` without
/// a way to print what is stored.
#[derive(Clone)]
pub struct SecretStore(Arc<dyn Keystore>);

impl SecretStore {
    /// Wraps any keystore -- tests pass a `MemoryKeystore`.
    pub fn new(keystore: Arc<dyn Keystore>) -> Self {
        Self(keystore)
    }

    /// The platform keystore (`whspr_config::OsKeystore`).
    pub fn os() -> Self {
        Self::new(Arc::new(OsKeystore::new()))
    }

    /// A store that forgets everything and says so
    /// (`Keystore::is_persistent` is false), so every secret stays in
    /// `config.toml` -- how the app behaves on a platform without a
    /// reboot-surviving keystore.
    pub fn plaintext_only() -> Self {
        Self::new(Arc::new(MemoryKeystore::non_persistent()))
    }

    /// The underlying keystore.
    pub fn keystore(&self) -> &dyn Keystore {
        self.0.as_ref()
    }
}

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStore")
            .field("persistent", &self.0.is_persistent())
            .finish_non_exhaustive()
    }
}

/// A secret the user manages from the Hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretSlot {
    /// A cloud backend's API key, by backend id (`"openai"`, ...).
    ApiKey(&'static str),
    /// The HuggingFace access token.
    HfToken,
}

impl SecretSlot {
    /// This secret's keystore entry name (see `whspr_config::SecretName`).
    fn keystore_name(self) -> String {
        match self {
            SecretSlot::ApiKey(backend_id) => SecretName::api_key(backend_id),
            SecretSlot::HfToken => SecretName::HF_TOKEN.to_string(),
        }
    }

    /// This secret's plaintext copy in `config`, if it has one.
    fn plaintext(self, config: &Config) -> Option<&String> {
        match self {
            SecretSlot::ApiKey(backend_id) => config.api_keys.get(backend_id),
            SecretSlot::HfToken => config.huggingface.token.as_ref(),
        }
    }

    /// Drops this secret's plaintext copy from `config`.
    fn clear_plaintext(self, config: &mut Config) {
        match self {
            SecretSlot::ApiKey(backend_id) => {
                config.api_keys.remove(backend_id);
            }
            SecretSlot::HfToken => config.huggingface.token = None,
        }
    }

    /// Writes `value` as this secret's plaintext copy in `config`.
    fn set_plaintext(self, config: &mut Config, value: &str) {
        match self {
            SecretSlot::ApiKey(backend_id) => {
                config
                    .api_keys
                    .insert(backend_id.to_string(), value.to_string());
            }
            SecretSlot::HfToken => config.huggingface.token = Some(value.to_string()),
        }
    }
}
