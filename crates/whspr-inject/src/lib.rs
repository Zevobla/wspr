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

/// Translates a `global-hotkey` press/release state into our own
/// `HotkeyEvent`. Split out as a pure function so the translation can be
/// unit tested without needing a real OS-level hotkey to fire.
fn map_hotkey_state(state: HotKeyState) -> HotkeyEvent {
    match state {
        HotKeyState::Pressed => HotkeyEvent::Pressed,
        HotKeyState::Released => HotkeyEvent::Released,
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
                let hk_event = map_hotkey_state(event.state);

                // `blocking_send` is designed exactly for sending from a
                // synchronous, non-async thread into a tokio mpsc channel —
                // it doesn't require any ambient tokio runtime context on
                // this thread (unlike `Handle::try_current` + `block_on`,
                // which fails here since this is a plain `std::thread`).
                if tx.blocking_send(hk_event).is_err() {
                    // Receiver dropped, stop listening
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

    /// Decides which injection strategy `insert` should use for `text`.
    /// Split out as a pure function so the branching can be unit tested
    /// without actually driving enigo or the clipboard.
    fn use_clipboard_paste(text: &str) -> bool {
        text.len() > Self::LONG_TEXT_THRESHOLD
    }
}

impl TextSink for EnigoTextSink {
    fn insert(&self, text: &str) -> Result<()> {
        if Self::use_clipboard_paste(text) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Regression test for a bug where `subscribe()`'s background thread used
    /// `tokio::runtime::Handle::try_current()` to send events, which fails
    /// immediately on a plain `std::thread` with no ambient tokio runtime
    /// context — causing the thread to exit right away, drop `tx`, and close
    /// the channel before any hotkey event could ever be forwarded. We can't
    /// easily fire a real OS-level hotkey in a test, but we can assert the
    /// background thread stays alive (channel stays open) instead of dying
    /// immediately after `subscribe()` is called.
    #[test]
    fn subscribe_keeps_channel_open_without_immediate_close() {
        let listener = match GlobalHotkeyListener::new() {
            Ok(l) => l,
            Err(e) => {
                // Some sandboxed/CI environments (no display server, no
                // accessibility permissions, etc.) can't register a global
                // hotkey at all. That's an environment limitation, not a
                // logic bug, so skip rather than fail.
                eprintln!("skipping test: failed to create GlobalHotkeyListener: {e}");
                return;
            }
        };

        let mut rx = listener.subscribe();

        // Give the background thread time to start and, under the old buggy
        // implementation, hit its immediate early-return.
        thread::sleep(Duration::from_millis(200));

        match rx.try_recv() {
            Err(mpsc::error::TryRecvError::Disconnected) => {
                panic!(
                    "subscribe() background thread exited immediately - \
                     channel closed with no hotkey event ever received"
                );
            }
            _ => {
                // Empty (no real hotkey fired, as expected in a test) or
                // Ok(event) both mean the background thread is alive.
            }
        }
    }

    #[test]
    fn map_hotkey_state_translates_pressed_and_released() {
        assert_eq!(map_hotkey_state(HotKeyState::Pressed), HotkeyEvent::Pressed);
        assert_eq!(
            map_hotkey_state(HotKeyState::Released),
            HotkeyEvent::Released
        );
    }

    #[test]
    fn use_clipboard_paste_switches_at_the_length_threshold() {
        let at_threshold = "x".repeat(EnigoTextSink::LONG_TEXT_THRESHOLD);
        let over_threshold = "x".repeat(EnigoTextSink::LONG_TEXT_THRESHOLD + 1);

        assert!(!EnigoTextSink::use_clipboard_paste(""));
        assert!(!EnigoTextSink::use_clipboard_paste(&at_threshold));
        assert!(EnigoTextSink::use_clipboard_paste(&over_threshold));
    }

    /// Verifies the arboard integration is real: setting text actually
    /// round-trips through the system clipboard, including non-ASCII text
    /// (the case `paste_from_clipboard` exists for). This is the one piece
    /// of `EnigoTextSink` that's exercisable without synthesizing keystrokes,
    /// since it only touches the clipboard, not the focused window.
    ///
    /// Side effect: this overwrites the real system clipboard. We save and
    /// best-effort restore whatever was there before.
    #[test]
    fn clipboard_round_trip_preserves_unicode_text() {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                // No clipboard/display server available (e.g. headless CI).
                eprintln!("skipping test: no clipboard access in this environment: {e}");
                return;
            }
        };

        let previous = clipboard.get_text().ok();

        let payload = "hello, world — café ☕ 日本語";
        clipboard
            .set_text(payload)
            .expect("set_text should succeed once clipboard access is available");
        let read_back = clipboard
            .get_text()
            .expect("get_text should succeed right after set_text");
        assert_eq!(read_back, payload);

        if let Some(previous) = previous {
            let _ = clipboard.set_text(previous);
        }
    }

    /// `EnigoTextSink::insert` for short text synthesizes real keystrokes
    /// via enigo into whatever window currently has OS focus. That needs an
    /// active display session and (on macOS) Accessibility permission
    /// granted to the test process, and it types into whatever happens to
    /// be focused when the test runs — not something to fire unattended in
    /// CI. Kept here, `#[ignore]`d, so a developer with a real desktop
    /// session and a scratch text field focused can run it deliberately via
    /// `cargo test -p whspr-inject -- --ignored`.
    #[test]
    #[ignore = "types real keystrokes into whatever window has OS focus; needs a display + Accessibility permission, run manually"]
    fn type_text_sends_real_keystrokes() {
        let sink = EnigoTextSink;
        sink.insert("whspr-inject manual keystroke test")
            .expect("insert should succeed with a display and Accessibility permission granted");
    }

    /// `EnigoTextSink::insert` for long text sets the clipboard (tested
    /// above) and then simulates a real paste keystroke (Cmd+V / Ctrl+V)
    /// into whatever window has OS focus. Same non-hermetic constraints as
    /// `type_text_sends_real_keystrokes` above.
    #[test]
    #[ignore = "pastes into whatever window has OS focus; needs a display + Accessibility permission, run manually"]
    fn paste_from_clipboard_sends_real_paste_keystroke() {
        let sink = EnigoTextSink;
        let long_text = "x".repeat(EnigoTextSink::LONG_TEXT_THRESHOLD + 1);
        sink.insert(&long_text)
            .expect("insert should succeed with a display and Accessibility permission granted");
    }
}
