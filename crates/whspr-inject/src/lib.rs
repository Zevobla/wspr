//! Global hotkey listening and text injection. Implements
//! `whspr_core::HotkeyListener` and `whspr_core::TextSink`; bodies are
//! `todo!()` until the inject team opts in `global-hotkey`/`enigo`/`arboard`
//! from this crate's own Cargo.toml — this crate must keep compiling without
//! those in the meantime.

use tokio::sync::mpsc;

use whspr_core::{HotkeyEvent, HotkeyListener, Result, TextSink};

/// Listens for the configured global hotkey via the OS-level hotkey APIs.
pub struct GlobalHotkeyListener;

impl HotkeyListener for GlobalHotkeyListener {
    fn subscribe(&self) -> mpsc::Receiver<HotkeyEvent> {
        todo!("whspr-inject: wire up global-hotkey and forward press/release events")
    }
}

/// Delivers text to the focused application via synthetic keystrokes
/// (falling back to clipboard paste for long text).
pub struct EnigoTextSink;

impl TextSink for EnigoTextSink {
    fn insert(&self, text: &str) -> Result<()> {
        let _ = text;
        todo!("whspr-inject: wire up enigo keystrokes / arboard clipboard paste")
    }
}
