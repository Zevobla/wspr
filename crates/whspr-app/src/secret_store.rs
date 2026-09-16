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

/// Where a managed secret currently lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretLocation {
    /// In the OS keystore.
    Keystore,
    /// In `config.toml`, in plaintext.
    ConfigFile,
    /// Not set anywhere.
    Missing,
}

/// What the platform calls its keystore, for UI copy.
pub fn keystore_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Credential Manager"
    } else {
        "Keychain"
    }
}

/// Finds where `slot` lives. The keystore wins -- it is what
/// `Config::resolve_api_key`/`resolve_hf_token` read first -- but is only
/// consulted when it survives a reboot (anywhere else nothing is ever put
/// there); an unreadable keystore counts as "not there".
pub fn locate_secret(config: &Config, keystore: &dyn Keystore, slot: SecretSlot) -> SecretLocation {
    if keystore.is_persistent() && matches!(keystore.get(&slot.keystore_name()), Ok(Some(_))) {
        SecretLocation::Keystore
    } else if slot.plaintext(config).is_some() {
        SecretLocation::ConfigFile
    } else {
        SecretLocation::Missing
    }
}

/// Saves `value` as `slot`. With a keystore that survives a reboot it goes
/// there, and any plaintext copy is dropped from `config` so the next save
/// stops writing it to disk; otherwise it goes into `config` in plaintext,
/// exactly as before P-06. A keystore write error leaves `config` untouched.
pub fn store_secret(
    config: &mut Config,
    keystore: &dyn Keystore,
    slot: SecretSlot,
    value: &str,
) -> Result<SecretLocation, String> {
    if !keystore.is_persistent() {
        slot.set_plaintext(config, value);
        return Ok(SecretLocation::ConfigFile);
    }
    keystore
        .set(&slot.keystore_name(), value)
        .map_err(|error| format!("could not save to the {}: {error}", keystore_label()))?;
    slot.clear_plaintext(config);
    Ok(SecretLocation::Keystore)
}

/// Removes `slot` everywhere: its plaintext copy in `config` and, with a
/// persistent keystore, its keystore entry.
pub fn clear_secret(
    config: &mut Config,
    keystore: &dyn Keystore,
    slot: SecretSlot,
) -> Result<(), String> {
    slot.clear_plaintext(config);
    if keystore.is_persistent() {
        keystore.delete(&slot.keystore_name()).map_err(|error| {
            format!("could not remove it from the {}: {error}", keystore_label())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persistent() -> MemoryKeystore {
        MemoryKeystore::default()
    }

    #[test]
    fn a_persistent_keystore_takes_the_secret_and_scrubs_plaintext() {
        let mut config = Config::default();
        config
            .api_keys
            .insert("openai".into(), "old-plaintext".into());
        let keystore = persistent();
        let slot = SecretSlot::ApiKey("openai");

        let location = store_secret(&mut config, &keystore, slot, "sk-new").unwrap();

        assert_eq!(location, SecretLocation::Keystore);
        assert!(config.api_keys.get("openai").is_none());
        assert_eq!(
            config
                .resolve_api_key("openai", &keystore)
                .unwrap()
                .as_deref(),
            Some("sk-new")
        );
        assert_eq!(
            locate_secret(&config, &keystore, slot),
            SecretLocation::Keystore
        );
    }

    #[test]
    fn a_non_persistent_keystore_keeps_the_secret_in_config() {
        let mut config = Config::default();
        let keystore = MemoryKeystore::non_persistent();

        let location = store_secret(&mut config, &keystore, SecretSlot::HfToken, "hf_abc").unwrap();

        assert_eq!(location, SecretLocation::ConfigFile);
        assert_eq!(config.huggingface.token.as_deref(), Some("hf_abc"));
        assert_eq!(keystore.get(SecretName::HF_TOKEN).unwrap(), None);
        assert_eq!(
            locate_secret(&config, &keystore, SecretSlot::HfToken),
            SecretLocation::ConfigFile
        );
    }

    #[test]
    fn clearing_removes_the_secret_from_both_stores() {
        let mut config = Config::default();
        config.huggingface.token = Some("hf_plain".into());
        let keystore = persistent();
        keystore.set(SecretName::HF_TOKEN, "hf_stored").unwrap();

        clear_secret(&mut config, &keystore, SecretSlot::HfToken).unwrap();

        assert_eq!(
            locate_secret(&config, &keystore, SecretSlot::HfToken),
            SecretLocation::Missing
        );
    }
}
