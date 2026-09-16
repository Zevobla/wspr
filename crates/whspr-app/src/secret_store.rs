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
