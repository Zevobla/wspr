//! Moves plaintext secrets (`Config::api_keys`, `HuggingFaceSettings::token`)
//! into a [`Keystore`]. See `resolve.rs` for reading secrets back out
//! afterwards.

use whspr_core::WhsprError;

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
    /// All-or-nothing: every secret is written to `ks` and read back
    /// before *any* plaintext field is blanked. If a write fails, or the
    /// store doesn't return what was written, this returns `Err` and the
    /// config is left exactly as it was, so a later save can never drop a
    /// key that didn't make it into the keystore. Refuses outright (also
    /// `Err`, config untouched) when [`Keystore::is_persistent`] is
    /// `false`, e.g. keyutils on Linux, where "migrating" would lose the
    /// secrets on the next reboot.
    ///
    /// Idempotent: a second call finds nothing left in the plaintext
    /// fields (this call already blanked them) and returns an empty
    /// report. Never *deletes* a keystore entry -- a migrated value
    /// overwrites whatever was already in the keystore for that name,
    /// since the config file is the more recently edited source.
    ///
    /// Does not save -- the caller persists the now-blanked config (e.g.
    /// `config.save(dir)`).
    pub fn migrate_secrets_to_keystore(
        &mut self,
        ks: &dyn Keystore,
    ) -> whspr_core::Result<MigrationReport> {
        if !ks.is_persistent() {
            return Err(WhsprError::Config(
                "the OS keystore on this platform does not survive a reboot; \
                 secrets stay in config.toml"
                    .to_string(),
            ));
        }

        let mut pending: Vec<(String, String)> = self
            .api_keys
            .iter()
            .map(|(backend_id, key)| (SecretName::api_key(backend_id), key.clone()))
            .collect();
        pending.sort();
        if let Some(token) = &self.huggingface.token {
            pending.push((SecretName::HF_TOKEN.to_string(), token.clone()));
        }

        for (name, value) in &pending {
            ks.set(name, value)?;
            if ks.get(name)?.as_deref() != Some(value.as_str()) {
                return Err(WhsprError::Config(format!(
                    "keystore did not keep {name}; secrets stay in config.toml"
                )));
            }
        }

        self.api_keys.clear();
        self.huggingface.token = None;
        Ok(MigrationReport {
            moved: pending.into_iter().map(|(name, _)| name).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::secrets::test_support::FaultyKeystore;
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

    fn config_with_two_keys_and_a_token() -> Config {
        let mut cfg = Config::default();
        cfg.api_keys.insert("anthropic".into(), "sk-anthropic".into());
        cfg.api_keys.insert("openai".into(), "sk-openai".into());
        cfg.huggingface.token = Some("hf-abc".into());
        cfg
    }

    fn assert_plaintext_untouched(cfg: &Config) {
        assert_eq!(cfg.api_keys.len(), 2);
        assert_eq!(
            cfg.api_keys.get("openai").map(String::as_str),
            Some("sk-openai")
        );
        assert_eq!(cfg.huggingface.token.as_deref(), Some("hf-abc"));
    }

    #[test]
    fn migrate_refuses_a_non_persistent_keystore_and_keeps_plaintext() {
        let mut cfg = config_with_two_keys_and_a_token();
        let ks = MemoryKeystore::non_persistent();
        assert!(cfg.migrate_secrets_to_keystore(&ks).is_err());
        assert_plaintext_untouched(&cfg);
        assert_eq!(ks.get(&SecretName::api_key("openai")).unwrap(), None);
    }

    #[test]
    fn migrate_keeps_every_plaintext_secret_when_a_keystore_write_fails() {
        let mut cfg = config_with_two_keys_and_a_token();
        let ks = FaultyKeystore {
            fail_set_for: Some(SecretName::api_key("openai")),
            ..FaultyKeystore::default()
        };
        assert!(cfg.migrate_secrets_to_keystore(&ks).is_err());
        assert_plaintext_untouched(&cfg);
    }

    #[test]
    fn migrate_keeps_plaintext_when_the_keystore_does_not_read_back() {
        let mut cfg = config_with_two_keys_and_a_token();
        let ks = FaultyKeystore {
            drop_writes: true,
            ..FaultyKeystore::default()
        };
        assert!(cfg.migrate_secrets_to_keystore(&ks).is_err());
        assert_plaintext_untouched(&cfg);
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
