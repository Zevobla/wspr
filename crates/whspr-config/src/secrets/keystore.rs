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
}

/// An in-process, non-persistent [`Keystore`] double for tests -- and for
/// any environment where the platform keychain plain doesn't exist (e.g. a
/// CI runner with no Secret Service session). Values live only as long as
/// this struct does and never touch the OS.
#[derive(Debug, Default)]
pub struct MemoryKeystore {
    values: Mutex<HashMap<String, String>>,
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
}
