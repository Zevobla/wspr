//! OS-keystore-backed secret storage (criterion P-06): API keys, the
//! HuggingFace OAuth token, and the history-encryption key belong here
//! rather than in `Config`'s plaintext fields.
//!
//! [`Keystore`] is the storage abstraction; [`MemoryKeystore`] is an
//! in-process double for tests.

mod keystore;

pub use keystore::{Keystore, MemoryKeystore};
