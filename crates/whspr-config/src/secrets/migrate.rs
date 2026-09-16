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

#[cfg(test)]
mod tests {
    use crate::secrets::MemoryKeystore;
    use crate::Config;

    use super::*;

    #[test]
    fn migrate_moves_api_keys_and_hf_token_into_the_keystore() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "sk-openai".into());
        cfg.api_keys
            .insert("anthropic".into(), "sk-anthropic".into());
        cfg.huggingface.token = Some("hf-abc".into());
        let ks = MemoryKeystore::default();

        let report = cfg.migrate_secrets_to_keystore(&ks).unwrap();

        assert_eq!(report.moved.len(), 3);
        assert!(cfg.api_keys.is_empty());
        assert!(cfg.huggingface.token.is_none());
        assert_eq!(
            ks.get(&SecretName::api_key("openai")).unwrap(),
            Some("sk-openai".to_string())
        );
        assert_eq!(
            ks.get(&SecretName::api_key("anthropic")).unwrap(),
            Some("sk-anthropic".to_string())
        );
        assert_eq!(
            ks.get(SecretName::HF_TOKEN).unwrap(),
            Some("hf-abc".to_string())
        );
    }

    #[test]
    fn migrate_with_nothing_to_move_returns_an_empty_report() {
        let mut cfg = Config::default();
        let ks = MemoryKeystore::default();

        let report = cfg.migrate_secrets_to_keystore(&ks).unwrap();

        assert!(report.moved.is_empty());
    }

    #[test]
    fn migrate_is_idempotent_and_never_deletes_a_keystore_entry() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "sk-openai".into());
        let ks = MemoryKeystore::default();

        let first = cfg.migrate_secrets_to_keystore(&ks).unwrap();
        let second = cfg.migrate_secrets_to_keystore(&ks).unwrap();

        assert_eq!(first.moved.len(), 1);
        assert!(second.moved.is_empty());
        // The first migration's value is still there.
        assert_eq!(
            ks.get(&SecretName::api_key("openai")).unwrap(),
            Some("sk-openai".to_string())
        );
    }

    #[test]
    fn migrate_then_save_round_trip_has_no_plaintext_secrets_in_the_file() {
        let mut cfg = Config::default();
        cfg.api_keys.insert("openai".into(), "sk-openai".into());
        cfg.huggingface.token = Some("hf-abc".into());
        let ks = MemoryKeystore::default();
        cfg.migrate_secrets_to_keystore(&ks).unwrap();

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        cfg.save(dir.path()).expect("failed to save config");
        let contents = std::fs::read_to_string(dir.path().join("config.toml"))
            .expect("failed to read config.toml");

        assert!(!contents.contains("sk-openai"));
        assert!(!contents.contains("hf-abc"));

        // The reload picks up the now-blank plaintext fields, but
        // resolving through the same keystore still finds the migrated
        // values.
        let reloaded = crate::load_from(Some(dir.path()));
        assert!(reloaded.api_keys.is_empty());
        assert!(reloaded.huggingface.token.is_none());
        assert_eq!(
            reloaded.resolve_api_key("openai", &ks).unwrap(),
            Some("sk-openai".to_string())
        );
        assert_eq!(
            reloaded.resolve_hf_token(&ks).unwrap(),
            Some("hf-abc".to_string())
        );
    }
}
