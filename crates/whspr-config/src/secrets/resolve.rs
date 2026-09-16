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
    pub fn resolve_api_key(
        &self,
        backend_id: &str,
        ks: &dyn Keystore,
    ) -> whspr_core::Result<Option<String>> {
        if let Some(key) = ks.get(&SecretName::api_key(backend_id))? {
            return Ok(Some(key));
        }
        Ok(self.api_keys.get(backend_id).cloned())
    }

    /// Resolves the HuggingFace OAuth token the same way
    /// [`resolve_api_key`](Self::resolve_api_key) resolves a backend's API
    /// key: the keystore first, then the legacy plaintext
    /// `huggingface.token` field.
    pub fn resolve_hf_token(&self, ks: &dyn Keystore) -> whspr_core::Result<Option<String>> {
        if let Some(token) = ks.get(SecretName::HF_TOKEN)? {
            return Ok(Some(token));
        }
        Ok(self.huggingface.token.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::secrets::MemoryKeystore;
    use crate::Config;

    use super::*;

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
