//! The embedded app payload: the `whspr-app` binary -- and, on x64, its
//! adjacent DLLs -- baked into this installer so it ships as a single
//! self-contained `.exe`.
//!
//! `PAYLOAD` is generated at build time by `build.rs` from the directory named
//! by `WHSPR_APP_PAYLOAD_DIR` (the oracle stages the app bundle there at
//! release-build time). When that env var is unset -- the macOS gate, CI
//! compile-checks, and the screenshot harness -- the list is empty and the
//! installer still builds and renders every screen; `crate::install::plan`
//! then simply skips the file-copy step. `crate::install` writes each
//! `(name, bytes)` entry into `%LOCALAPPDATA%\whspr` on the real Windows path.

include!(concat!(env!("OUT_DIR"), "/payload_generated.rs"));
