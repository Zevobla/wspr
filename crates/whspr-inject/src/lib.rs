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

/// A minimal system-clipboard abstraction.
///
/// The clipboard-paste path needs to *save* the user's current clipboard,
/// stage our own text, paste, then *restore* what was there before. Hiding
/// the real `arboard` behind this trait lets that save/set/restore sequence
/// be unit-tested with an in-memory fake, without a display server or a
/// live system clipboard.
trait Clipboard {
    /// Returns the current clipboard text, or an error if there is no text
    /// on the clipboard (e.g. it's empty or holds a non-text payload).
    fn get_text(&mut self) -> Result<String>;
    /// Replaces the clipboard contents with `text`.
    fn set_text(&mut self, text: &str) -> Result<()>;
    /// Empties the clipboard.
    fn clear(&mut self) -> Result<()>;
}

/// The real [`Clipboard`], backed by the system clipboard via `arboard`.
struct ArboardClipboard(arboard::Clipboard);

impl ArboardClipboard {
    /// Opens a handle to the system clipboard.
    fn new() -> Result<Self> {
        arboard::Clipboard::new()
            .map(ArboardClipboard)
            .map_err(|e| WhsprError::Inject(format!("failed to access clipboard: {}", e)))
    }
}

impl Clipboard for ArboardClipboard {
    fn get_text(&mut self) -> Result<String> {
        self.0
            .get_text()
            .map_err(|e| WhsprError::Inject(format!("failed to read clipboard: {}", e)))
    }

    fn set_text(&mut self, text: &str) -> Result<()> {
        self.0
            .set_text(text)
            .map_err(|e| WhsprError::Inject(format!("failed to set clipboard: {}", e)))
    }

    fn clear(&mut self) -> Result<()> {
        self.0
            .clear()
            .map_err(|e| WhsprError::Inject(format!("failed to clear clipboard: {}", e)))
    }
}

/// Restores a saved clipboard state when dropped.
///
/// Holding the restore in a guard makes it run no matter how the paste
/// step turns out — a normal return, an early error, or even a panic
/// unwinding through the paste — so we never leave our injected text
/// sitting on the user's clipboard.
struct ClipboardRestoreGuard<'a, C: Clipboard> {
    clipboard: &'a mut C,
    /// What was on the clipboard before we overwrote it. `None` means it
    /// held no text (empty, or a non-text payload we can't reproduce), so
    /// the closest restore is to clear it.
    original: Option<String>,
}

impl<'a, C: Clipboard> ClipboardRestoreGuard<'a, C> {
    fn new(clipboard: &'a mut C, original: Option<String>) -> Self {
        Self {
            clipboard,
            original,
        }
    }
}

impl<C: Clipboard> Drop for ClipboardRestoreGuard<'_, C> {
    fn drop(&mut self) {
        // Best-effort: if restoring fails there's nothing useful we can do
        // during unwinding, so swallow the error rather than risk a panic.
        let _ = match &self.original {
            Some(text) => self.clipboard.set_text(text),
            None => self.clipboard.clear(),
        };
    }
}

/// What happened when we tried to inject via the clipboard.
enum PasteOutcome {
    /// Our text was staged on the clipboard and the paste ran; carries the
    /// paste's own result (which may itself be an error). The original
    /// clipboard has already been restored.
    Pasted(Result<()>),
    /// We couldn't even stage our text on the clipboard (access denied,
    /// etc.), so the caller should fall back to another injection method.
    /// The clipboard was left untouched.
    Unstaged,
}

/// Saves the current clipboard, stages `text`, runs `paste`, then restores
/// the original clipboard.
///
/// The restore happens via a [`ClipboardRestoreGuard`], so the user's
/// clipboard is put back even if `paste` returns an error or panics. If our
/// text can't be staged in the first place, returns [`PasteOutcome::Unstaged`]
/// without touching the clipboard, leaving the caller to fall back.
fn stage_and_paste<C, F>(clipboard: &mut C, text: &str, paste: F) -> PasteOutcome
where
    C: Clipboard,
    F: FnOnce() -> Result<()>,
{
    // Save whatever is on the clipboard now so we can put it back later.
    let original = clipboard.get_text().ok();

    if clipboard.set_text(text).is_err() {
        return PasteOutcome::Unstaged;
    }

    // From here on the clipboard holds our text; the guard restores the
    // saved contents when this scope ends, however `paste` turns out.
    let _guard = ClipboardRestoreGuard::new(clipboard, original);
    PasteOutcome::Pasted(paste())
}

/// Delivers text to the focused application via synthetic keystrokes
/// (falling back to clipboard paste for long text).
pub struct EnigoTextSink;

impl EnigoTextSink {
    /// The threshold (in characters) above which we switch from keystrokes to clipboard paste
    const LONG_TEXT_THRESHOLD: usize = 200;

    /// How long to wait after sending the paste keystroke before restoring
    /// the user's clipboard. The synthesized Cmd+V/Ctrl+V is delivered
    /// asynchronously by the OS, so we give the target app a moment to read
    /// the clipboard before putting the original contents back.
    const PASTE_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(120);

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

    /// Copies text to the clipboard and simulates a paste keystroke,
    /// restoring the user's previous clipboard contents afterward.
    ///
    /// If the clipboard can't be used (no display, permission denied, a
    /// non-text payload we can't stage over, etc.) this falls back to
    /// typing the text directly rather than failing the injection outright.
    fn paste_from_clipboard(&self, text: &str) -> Result<()> {
        // If we can't even open the clipboard, type the text instead.
        let mut clipboard = match ArboardClipboard::new() {
            Ok(clipboard) => clipboard,
            Err(_) => return self.type_text(text),
        };

        match stage_and_paste(&mut clipboard, text, || self.send_paste_keystroke()) {
            PasteOutcome::Pasted(result) => result,
            // Couldn't stage our text on the clipboard; type it instead.
            PasteOutcome::Unstaged => self.type_text(text),
        }
    }

    /// Simulates the platform paste shortcut (Cmd+V on macOS, Ctrl+V
    /// elsewhere) into whatever window currently has focus.
    fn send_paste_keystroke(&self) -> Result<()> {
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

        // Let the target consume the clipboard before the caller restores it.
        thread::sleep(Self::PASTE_SETTLE_DELAY);

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

    /// An in-memory [`Clipboard`] test double, so the save/stage/restore
    /// sequence can be exercised with no display server or real clipboard.
    struct MockClipboard {
        /// Current clipboard contents (`None` = empty / no text payload).
        content: Option<String>,
        /// When true, every `set_text` fails, simulating a denied clipboard.
        set_fails: bool,
        /// Every value passed to `set_text`, in order — lets tests assert
        /// the exact stage-then-restore sequence.
        writes: Vec<String>,
    }

    impl MockClipboard {
        fn with_content(content: Option<&str>) -> Self {
            Self {
                content: content.map(str::to_string),
                set_fails: false,
                writes: Vec::new(),
            }
        }
    }

    impl Clipboard for MockClipboard {
        fn get_text(&mut self) -> Result<String> {
            self.content
                .clone()
                .ok_or_else(|| WhsprError::Inject("clipboard is empty".into()))
        }

        fn set_text(&mut self, text: &str) -> Result<()> {
            if self.set_fails {
                return Err(WhsprError::Inject("clipboard access denied".into()));
            }
            self.content = Some(text.to_string());
            self.writes.push(text.to_string());
            Ok(())
        }

        fn clear(&mut self) -> Result<()> {
            self.content = None;
            Ok(())
        }
    }

    /// The happy path: stage our text, run the paste, then put the user's
    /// original clipboard back — in that order.
    #[test]
    fn stage_and_paste_restores_original_after_successful_paste() {
        let mut clipboard = MockClipboard::with_content(Some("user's data"));

        let outcome = stage_and_paste(&mut clipboard, "injected text", || Ok(()));

        assert!(matches!(outcome, PasteOutcome::Pasted(Ok(()))));
        // Our text was staged first, then the original was restored.
        let writes: Vec<&str> = clipboard.writes.iter().map(String::as_str).collect();
        assert_eq!(writes, ["injected text", "user's data"]);
        // The clipboard ends up back exactly where it started.
        assert_eq!(clipboard.content.as_deref(), Some("user's data"));
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

    /// End-to-end proof that a *real* OS-level key press actually reaches
    /// `subscribe()`'s receiver requires a human physically pressing the
    /// registered hotkey (Ctrl+Space) while the test is running — there's
    /// no way to synthesize that OS-level input event from within the test
    /// process itself. `subscribe_keeps_channel_open_without_immediate_close`
    /// above covers everything that's automatable; this documents the gap
    /// and gives a manual repro.
    #[test]
    #[ignore = "needs a human to physically press Ctrl+Space during the test; run manually"]
    fn subscribe_delivers_a_real_hotkey_press() {
        let listener = GlobalHotkeyListener::new().expect("failed to register global hotkey");
        let mut rx = listener.subscribe();
        eprintln!("press Ctrl+Space now...");
        let event = rx.blocking_recv().expect("channel closed with no event");
        assert_eq!(event, HotkeyEvent::Pressed);
    }
}
