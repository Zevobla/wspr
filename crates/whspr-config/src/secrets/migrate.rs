//! Moves plaintext secrets (`Config::api_keys`, `HuggingFaceSettings::token`)
//! into a [`Keystore`]. See `resolve.rs` for reading secrets back out
//! afterwards.

use crate::secrets::{Keystore, SecretName};
use crate::Config;

/// What [`Config::migrate_secrets_to_keystore`] moved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Keystore entry names ([`SecretName`]) that received a value moved
    /// out of the plaintext config during this call. Empty when there was
    /// nothing left to migrate.
    pub moved: Vec<String>,
}

impl Config {
    /// Copies every plaintext secret this config holds -- each `[api_keys]`
    /// entry and `huggingface.token` -- into `ks`, then blanks them in this
    /// struct so the caller's next `Config::save` no longer writes them to
    /// disk in plaintext.
    ///
    /// Idempotent: a second call finds nothing left in the plaintext
    /// fields (this call already blanked them) and returns an empty
    /// report. Never *deletes* a keystore entry -- a migrated value always
    /// overwrites whatever was already in the keystore for that name,
    /// since the config file is the more recently edited source, but this
    /// method has no delete path at all.
    ///
    /// Does not save -- the caller persists the now-blanked config (e.g.
    /// `config.save(dir)`).
    pub fn migrate_secrets_to_keystore(
        &mut self,
        ks: &dyn Keystore,
    ) -> whspr_core::Result<MigrationReport> {
        let mut moved = Vec::new();

        for (backend_id, key) in std::mem::take(&mut self.api_keys) {
            let name = SecretName::api_key(&backend_id);
            ks.set(&name, &key)?;
            moved.push(name);
        }

        if let Some(token) = self.huggingface.token.take() {
            ks.set(SecretName::HF_TOKEN, &token)?;
            moved.push(SecretName::HF_TOKEN.to_string());
        }

        Ok(MigrationReport { moved })
    }
}
