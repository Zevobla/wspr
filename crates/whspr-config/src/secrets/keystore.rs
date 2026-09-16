//! [`Keystore`]: a small abstraction over "somewhere outside the plaintext
//! config file to put a secret." [`MemoryKeystore`] is an in-process,
//! non-persistent double for tests (and this module's own doctests /
//! callers that don't want to touch the platform keychain). See
//! `keystore_os.rs` for [`super::OsKeystore`], the real backend.

use std::collections::HashMap;
use std::sync::Mutex;

use whspr_core::Result;

/// A place to store small secret strings outside the plaintext config
/// file. Implementations are keyed by an opaque name -- see
/// [`super::SecretName`] for the naming scheme whspr uses.
///
/// `get` on a name nothing has ever been `set` for returns `Ok(None)`,
/// never an error; only a genuine backend failure (a locked keychain, no
/// Secret Service session running, ...) should surface as `Err`. Likewise,
/// `delete` of an absent entry is not an error -- it's already the state
/// the caller wanted.
pub trait Keystore: Send + Sync {
    /// Reads a secret by name. `Ok(None)` means "no such entry".
    fn get(&self, name: &str) -> Result<Option<String>>;
    /// Writes (creating or overwriting) a secret by name.
    fn set(&self, name: &str, value: &str) -> Result<()>;
    /// Removes a secret by name. Deleting an absent entry is not an error.
    fn delete(&self, name: &str) -> Result<()>;

    /// Whether values written here survive an app restart *and* a reboot.
    /// Moving a secret out of `config.toml` into a store that forgets it
    /// would silently lose the user's API keys (and make encrypted history
    /// unreadable), so [`crate::Config::migrate_secrets_to_keystore`] and
    /// [`super::history_key`] refuse to run against a store that answers
    /// `false`. Defaults to `true`; stores that know better override it.
    fn is_persistent(&self) -> bool {
        true
    }
}

/// An in-process [`Keystore`] double for tests -- and for any environment
/// where the platform keychain doesn't exist (e.g. a CI runner with no
/// Secret Service session). Values live only as long as this struct does
/// and never touch the OS.
///
/// [`MemoryKeystore::default`] *simulates* a persistent keychain
/// ([`Keystore::is_persistent`] is `true`) so migration and history-key
/// logic can be exercised; [`MemoryKeystore::non_persistent`] simulates a
/// store that forgets on reboot, to test the refusal paths.
#[derive(Debug, Default)]
pub struct MemoryKeystore {
    values: Mutex<HashMap<String, String>>,
    non_persistent: bool,
}

impl MemoryKeystore {
    /// A memory keystore that reports [`Keystore::is_persistent`] as
    /// `false`, like a platform backend that loses entries on reboot.
    pub fn non_persistent() -> Self {
        Self {
            non_persistent: true,
            ..Self::default()
        }
    }
}

impl Keystore for MemoryKeystore {
    fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .values
            .lock()
            .expect("MemoryKeystore mutex poisoned")
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.values
            .lock()
            .expect("MemoryKeystore mutex poisoned")
            .insert(name.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.values
            .lock()
            .expect("MemoryKeystore mutex poisoned")
            .remove(name);
        Ok(())
    }

    fn is_persistent(&self) -> bool {
        !self.non_persistent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_on_missing_entry_is_none_not_an_error() {
        let ks = MemoryKeystore::default();
        assert_eq!(ks.get("nope").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips() {
        let ks = MemoryKeystore::default();
        ks.set("k", "v").unwrap();
        assert_eq!(ks.get("k").unwrap(), Some("v".to_string()));
    }

    #[test]
    fn set_overwrites_an_existing_value() {
        let ks = MemoryKeystore::default();
        ks.set("k", "v1").unwrap();
        ks.set("k", "v2").unwrap();
        assert_eq!(ks.get("k").unwrap(), Some("v2".to_string()));
    }

    #[test]
    fn delete_removes_the_value() {
        let ks = MemoryKeystore::default();
        ks.set("k", "v").unwrap();
        ks.delete("k").unwrap();
        assert_eq!(ks.get("k").unwrap(), None);
    }

    #[test]
    fn delete_of_an_absent_entry_is_not_an_error() {
        let ks = MemoryKeystore::default();
        assert!(ks.delete("nope").is_ok());
    }

    #[test]
    fn default_memory_keystore_simulates_a_persistent_store() {
        assert!(MemoryKeystore::default().is_persistent());
    }

    #[test]
    fn non_persistent_memory_keystore_says_so() {
        assert!(!MemoryKeystore::non_persistent().is_persistent());
    }

    #[test]
    fn entries_are_independent_by_name() {
        let ks = MemoryKeystore::default();
        ks.set("a", "1").unwrap();
        ks.set("b", "2").unwrap();
        assert_eq!(ks.get("a").unwrap(), Some("1".to_string()));
        assert_eq!(ks.get("b").unwrap(), Some("2".to_string()));
    }
}
