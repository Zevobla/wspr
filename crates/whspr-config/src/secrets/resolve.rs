//! `Config` methods that read secrets through a [`Keystore`], preferring it
//! over the legacy plaintext `[api_keys]` table / `huggingface.token`
//! field. See `migrate.rs` for moving the plaintext values into the
//! keystore.

use crate::secrets::{Keystore, SecretName};
use crate::Config;

impl Config {
    /// Resolves the API key for `backend_id` (e.g. `"openai"`, matching
    /// `AsrBackend::id()` / `TextRefiner::id()`): the keystore first, then
    /// the legacy plaintext `[api_keys]` table as a fallback for a config
    /// that hasn't been migrated yet (see
    /// [`migrate_secrets_to_keystore`](Self::migrate_secrets_to_keystore)).
    /// `Ok(None)` means neither source has a key for this backend.
    ///
    /// A keystore *read error* (locked keychain, no credential service)
    /// falls back to the plaintext value when there is one, so an
    /// unmigrated config keeps working on a machine without a usable
    /// keychain; the error surfaces only when plaintext has nothing either.
    pub fn resolve_api_key(
        &self,
        backend_id: &str,
        ks: &dyn Keystore,
    ) -> whspr_core::Result<Option<String>> {
        let plaintext = self.api_keys.get(backend_id).cloned();
        prefer_keystore(ks.get(&SecretName::api_key(backend_id)), plaintext)
    }

    /// Resolves the HuggingFace OAuth token the same way
    /// [`resolve_api_key`](Self::resolve_api_key) resolves a backend's API
    /// key: the keystore first, then the legacy plaintext
    /// `huggingface.token` field.
    pub fn resolve_hf_token(&self, ks: &dyn Keystore) -> whspr_core::Result<Option<String>> {
        prefer_keystore(ks.get(SecretName::HF_TOKEN), self.huggingface.token.clone())
    }
}

/// Keystore value if present; otherwise the plaintext fallback. A keystore
/// error is swallowed only when the fallback actually has a value.
fn prefer_keystore(
    from_keystore: whspr_core::Result<Option<String>>,
    plaintext: Option<String>,
) -> whspr_core::Result<Option<String>> {
    match (from_keystore, plaintext) {
        (Ok(Some(value)), _) => Ok(Some(value)),
        (Ok(None), plaintext) => Ok(plaintext),
        (Err(_), Some(plaintext)) => Ok(Some(plaintext)),
        (Err(e), None) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use crate::secrets::test_support::FaultyKeystore;
    use crate::secrets::MemoryKeystore;
    use crate::Config;

    use super::*;

    fn unreadable_keystore() -> FaultyKeystore {
        FaultyKeystore {
            fail_get: true,
            ..FaultyKeystore::default()
        }
    }

    #[test]
    fn resolve_api_key_falls_back_to_plaintext_when_the_keystore_errors() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "plaintext-key".into());
        assert_eq!(
            cfg.resolve_api_key("openai", &unreadable_keystore())
                .unwrap(),
            Some("plaintext-key".to_string())
        );
    }

    #[test]
    fn resolve_api_key_surfaces_the_keystore_error_when_plaintext_is_empty() {
        let cfg = Config::default();
        assert!(cfg
            .resolve_api_key("openai", &unreadable_keystore())
            .is_err());
    }

    #[test]
    fn resolve_hf_token_falls_back_to_plaintext_when_the_keystore_errors() {
        let mut cfg = Config::default();
        cfg.huggingface.token = Some("hf-plain".into());
        assert_eq!(
            cfg.resolve_hf_token(&unreadable_keystore()).unwrap(),
            Some("hf-plain".to_string())
        );
    }

    #[test]
    fn resolve_api_key_prefers_keystore_over_plaintext() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "plaintext-key".into());
        let ks = MemoryKeystore::default();
        ks.set(&SecretName::api_key("openai"), "keystore-key")
            .unwrap();

        assert_eq!(
            cfg.resolve_api_key("openai", &ks).unwrap(),
            Some("keystore-key".to_string())
        );
    }

    #[test]
    fn resolve_api_key_falls_back_to_plaintext_when_keystore_empty() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "plaintext-key".into());
        let ks = MemoryKeystore::default();

        assert_eq!(
            cfg.resolve_api_key("openai", &ks).unwrap(),
            Some("plaintext-key".to_string())
        );
    }

    #[test]
    fn resolve_api_key_is_none_when_neither_source_has_it() {
        let cfg = Config::default();
        let ks = MemoryKeystore::default();
        assert_eq!(cfg.resolve_api_key("openai", &ks).unwrap(), None);
    }

    #[test]
    fn resolve_hf_token_prefers_keystore_over_plaintext() {
        let mut cfg = Config::default();
        cfg.huggingface.token = Some("plaintext-token".into());
        let ks = MemoryKeystore::default();
        ks.set(SecretName::HF_TOKEN, "keystore-token").unwrap();

        assert_eq!(
            cfg.resolve_hf_token(&ks).unwrap(),
            Some("keystore-token".to_string())
        );
    }

    #[test]
    fn resolve_hf_token_falls_back_to_plaintext_when_keystore_empty() {
        let mut cfg = Config::default();
        cfg.huggingface.token = Some("plaintext-token".into());
        let ks = MemoryKeystore::default();

        assert_eq!(
            cfg.resolve_hf_token(&ks).unwrap(),
            Some("plaintext-token".to_string())
        );
    }

    #[test]
    fn resolve_hf_token_is_none_when_neither_source_has_it() {
        let cfg = Config::default();
        let ks = MemoryKeystore::default();
        assert_eq!(cfg.resolve_hf_token(&ks).unwrap(), None);
    }
}
