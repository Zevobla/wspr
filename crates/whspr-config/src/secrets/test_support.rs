//! Keystore doubles that misbehave on purpose, for the failure-path tests
//! in `migrate.rs`, `resolve.rs` and `history.rs`. Test-only.

use whspr_core::{Result, WhsprError};

use super::{Keystore, MemoryKeystore};

/// A [`MemoryKeystore`] wrapper with switchable faults.
#[derive(Default)]
pub(crate) struct FaultyKeystore {
    inner: MemoryKeystore,
    /// `set` for this exact name returns an error.
    pub(crate) fail_set_for: Option<String>,
    /// Every `get` returns an error (a locked or unavailable keychain).
    pub(crate) fail_get: bool,
    /// `set` reports success but stores nothing.
    pub(crate) drop_writes: bool,
}

impl Keystore for FaultyKeystore {
    fn get(&self, name: &str) -> Result<Option<String>> {
        if self.fail_get {
            return Err(WhsprError::Config(format!("simulated get failure: {name}")));
        }
        self.inner.get(name)
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        if self.fail_set_for.as_deref() == Some(name) {
            return Err(WhsprError::Config(format!("simulated set failure: {name}")));
        }
        if self.drop_writes {
            return Ok(());
        }
        self.inner.set(name, value)
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.inner.delete(name)
    }
}
