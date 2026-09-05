//! Global hotkey listening and text injection. Implements
//! `whspr_core::HotkeyListener` and `whspr_core::TextSink`.

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc;

use whspr_core::{HotkeyEvent, HotkeyListener, Result, TextSink, WhsprError};

/// Listens for the configured global hotkey via the OS-level hotkey APIs.
pub struct GlobalHotkeyListener {
    // We store the manager to keep it alive for the lifetime of the listener
    _manager: Arc<GlobalHotKeyManager>,
}

impl GlobalHotkeyListener {
    /// Creates a new global hotkey listener with a default hotkey (Ctrl+Space).
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new().map_err(|e| {
            WhsprError::Inject(format!("failed to create global hotkey manager: {}", e))
        })?;

        // Register a default hotkey: Ctrl+Space
        let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);

        manager
            .register(hotkey)
            .map_err(|e| WhsprError::Inject(format!("failed to register global hotkey: {}", e)))?;

        Ok(GlobalHotkeyListener {
            _manager: Arc::new(manager),
        })
    }
}

impl Default for GlobalHotkeyListener {
    fn default() -> Self {
        Self::new().expect("failed to initialize GlobalHotkeyListener")
    }
}

impl HotkeyListener for GlobalHotkeyListener {
    fn subscribe(&self) -> mpsc::Receiver<HotkeyEvent> {
        let (tx, rx) = mpsc::channel(10);

        // Spawn a background thread that listens to global hotkey events
        // We use a separate thread because global_hotkey uses crossbeam channels
        thread::spawn(move || {
            // Get the global hotkey event receiver
            let receiver = GlobalHotKeyEvent::receiver();

            while let Ok(event) = receiver.recv() {
                let hk_event = match event.state {
                    HotKeyState::Pressed => HotkeyEvent::Pressed,
                    HotKeyState::Released => HotkeyEvent::Released,
                };

                // Send to tokio mpsc channel
                // We need to use block_on to run async code from a thread
                let runtime = tokio::runtime::Handle::try_current();
                if let Ok(rt) = runtime {
                    if rt.block_on(tx.send(hk_event)).is_err() {
                        // Receiver dropped, stop listening
                        break;
                    }
                } else {
                    // Not in a tokio runtime context
                    break;
                }
            }
        });

        rx
    }
}

/// Delivers text to the focused application via synthetic keystrokes
/// (falling back to clipboard paste for long text).
pub struct EnigoTextSink;

impl EnigoTextSink {
    /// The threshold (in characters) above which we switch from keystrokes to clipboard paste
    const LONG_TEXT_THRESHOLD: usize = 200;
}

impl TextSink for EnigoTextSink {
    fn insert(&self, text: &str) -> Result<()> {
        if text.len() > Self::LONG_TEXT_THRESHOLD {
            // For long text, use clipboard paste
            self.paste_from_clipboard(text)
        } else {
            // For short text, use direct keystrokes
            self.type_text(text)
        }
    }
}

impl EnigoTextSink {
    /// Types text directly using synthetic keystrokes.
    fn type_text(&self, text: &str) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| WhsprError::Inject(format!("failed to initialize enigo: {}", e)))?;

        enigo
            .text(text)
            .map_err(|e| WhsprError::Inject(format!("failed to type text: {}", e)))?;

        Ok(())
    }

    /// Copies text to clipboard and simulates a paste keystroke.
    fn paste_from_clipboard(&self, text: &str) -> Result<()> {
        // Set clipboard content
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| WhsprError::Inject(format!("failed to access clipboard: {}", e)))?;

        clipboard
            .set_text(text)
            .map_err(|e| WhsprError::Inject(format!("failed to set clipboard: {}", e)))?;

        // Simulate paste keystroke (Cmd+V on macOS, Ctrl+V elsewhere)
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| WhsprError::Inject(format!("failed to initialize enigo: {}", e)))?;

        #[cfg(target_os = "macos")]
        {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| WhsprError::Inject(format!("failed to press meta key: {}", e)))?;
            enigo
                .key(Key::Unicode('v'), Direction::Click)
                .map_err(|e| WhsprError::Inject(format!("failed to click v key: {}", e)))?;
            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| WhsprError::Inject(format!("failed to release meta key: {}", e)))?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| WhsprError::Inject(format!("failed to press control key: {}", e)))?;
            enigo
                .key(Key::Unicode('v'), Direction::Click)
                .map_err(|e| WhsprError::Inject(format!("failed to click v key: {}", e)))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| WhsprError::Inject(format!("failed to release control key: {}", e)))?;
        }

        Ok(())
    }
}
