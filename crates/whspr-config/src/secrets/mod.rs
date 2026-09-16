//! OS-keystore-backed secret storage (criterion P-06): API keys, the
//! HuggingFace OAuth token, and the history-encryption key belong here
//! rather than in `Config`'s plaintext fields.
//!
//! [`Keystore`] is the storage abstraction: [`OsKeystore`] for the real
//! platform keychain, [`MemoryKeystore`] for tests.

mod keystore;
mod keystore_os;
mod names;
mod resolve;

pub use keystore::{Keystore, MemoryKeystore};
pub use keystore_os::OsKeystore;
pub use names::SecretName;
