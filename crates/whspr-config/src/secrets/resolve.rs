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
