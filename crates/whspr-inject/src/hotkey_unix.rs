//! Non-Windows (macOS/Linux) global-hotkey listener.
//!
//! Off Windows, `GlobalHotKeyManager` is `Send + Sync`, so the listener owns
//! it directly and unregisters on drop. The Windows counterpart needs a
//! message-pump thread instead — see the `hotkey_windows` module.

use std::sync::Arc;

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::GlobalHotKeyManager;

use whspr_core::{Result, WhsprError};

use crate::default_hotkey_modifiers;

/// Listens for the configured global hotkey via the OS-level hotkey APIs.
///
/// Off Windows, `GlobalHotKeyManager` is `Send + Sync`, so the listener can
/// simply own it and unregister on drop. Windows needs a message-pump thread
/// instead — see the `hotkey_windows` module.
pub struct GlobalHotkeyListener {
    // Kept alive for the listener's lifetime, and used on drop to release
    // the hotkey.
    manager: Arc<GlobalHotKeyManager>,
    // The registered combo, remembered so `Drop` can unregister exactly it.
    hotkey: HotKey,
}

impl GlobalHotkeyListener {
    /// Creates a new global hotkey listener with the platform default hotkey
    /// (`Ctrl+Space`, or `Ctrl+Shift+Space` on Windows).
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new().map_err(|e| {
            WhsprError::Inject(format!("failed to create global hotkey manager: {}", e))
        })?;

        let hotkey = HotKey::new(Some(default_hotkey_modifiers()), Code::Space);

        manager
            .register(hotkey)
            .map_err(|e| WhsprError::Inject(format!("failed to register global hotkey: {}", e)))?;

        Ok(GlobalHotkeyListener {
            manager: Arc::new(manager),
            hotkey,
        })
    }
}

impl Default for GlobalHotkeyListener {
    fn default() -> Self {
        Self::new().expect("failed to initialize GlobalHotkeyListener")
    }
}

impl Drop for GlobalHotkeyListener {
    /// Releases the OS-level hotkey when the listener is dropped (app exit or
    /// teardown), so the combo isn't left registered with the system after
    /// the process goes away (D-13). Best-effort: a failure here isn't
    /// actionable during teardown and `Drop` must never panic, so the result
    /// is ignored.
    fn drop(&mut self) {
        let _ = self.manager.unregister(self.hotkey);
    }
}
