//! [`OsKeystore`]: the real, OS-native [`super::Keystore`] backend, via the
//! `keyring` crate. Never exercised by this crate's own tests (only
//! [`super::MemoryKeystore`] is) -- a unit test that touched the real
//! platform keychain would mutate the developer's own login keychain /
//! Credential Manager / Secret Service, which this project's test-hygiene
//! rules forbid.

use whspr_core::{Result, WhsprError};

use super::Keystore;

/// The service name every [`OsKeystore`] entry is filed under in the
/// platform keychain.
const SERVICE: &str = "whspr";

/// The real, OS-native keystore under the service name `"whspr"`: the
/// macOS Keychain or the Windows Credential Manager, both persistent. On
/// Linux the compiled-in `keyring` backend is kernel keyutils, which does
/// **not** survive a reboot, and on any other platform `keyring` falls back
/// to an in-memory mock -- so [`Keystore::is_persistent`] is `false` there
/// and callers keep secrets in `config.toml` instead.
#[derive(Debug, Default)]
pub struct OsKeystore;

impl OsKeystore {
    /// Creates a handle to the OS keystore. Cheap -- it doesn't touch the
    /// platform keychain until a `get`/`set`/`delete` call does.
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, name)
            .map_err(|e| WhsprError::Config(format!("keystore entry {name}: {e}")))
    }
}

impl Keystore for OsKeystore {
    fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(WhsprError::Config(format!("keystore get {name}: {e}"))),
        }
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.entry(name)?
            .set_password(value)
            .map_err(|e| WhsprError::Config(format!("keystore set {name}: {e}")))
    }

    fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(WhsprError::Config(format!("keystore delete {name}: {e}"))),
        }
    }

    fn is_persistent(&self) -> bool {
        os_keystore_is_persistent()
    }
}

/// `true` only on the platforms whose compiled-in `keyring` backend is a
/// real, reboot-surviving credential store (see [`OsKeystore`]'s doc).
fn os_keystore_is_persistent() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure platform check -- never touches the real keychain.
    #[test]
    fn os_keystore_persistence_matches_the_platform_backend() {
        let expected = cfg!(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "windows"
        ));
        assert_eq!(OsKeystore::new().is_persistent(), expected);
    }
}
