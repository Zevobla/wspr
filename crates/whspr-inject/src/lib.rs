//! Global hotkey listening and text injection. Implements
//! `whspr_core::HotkeyListener` and `whspr_core::TextSink`.

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tokio::sync::mpsc;

use whspr_core::{HotkeyEvent, HotkeyListener, Result, TextSink, WhsprError};

mod clipboard;
mod debounce;

use clipboard::{stage_and_paste, ArboardClipboard, PasteOutcome};

pub use debounce::{DebounceAction, DebouncedHotkeyListener, HotkeyDebouncer};

/// Listens for the configured global hotkey via the OS-level hotkey APIs.
pub struct GlobalHotkeyListener {
    // Kept alive for the listener's lifetime, and used on drop to release
    // the hotkey.
    manager: Arc<GlobalHotKeyManager>,
    // The registered combo, remembered so `Drop` can unregister exactly it.
    hotkey: HotKey,
}

impl GlobalHotkeyListener {
    /// Creates a new global hotkey listener with a default hotkey (Ctrl+Space).
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new().map_err(|e| {
            WhsprError::Inject(format!("failed to create global hotkey manager: {}", e))
        })?;

        let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);

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

/// Delivers text to the focused application by pasting via the clipboard —
/// the reliable default, since it lands text in editors, terminals, and vim
/// where raw synthetic keystrokes misbehave — falling back to typing only
/// when the clipboard is unavailable.
pub struct EnigoTextSink;

impl EnigoTextSink {
    /// How long to wait after sending the paste keystroke before restoring
    /// the user's clipboard. The synthesized Cmd+V/Ctrl+V is delivered
    /// asynchronously by the OS, so we give the target app a moment to read
    /// the clipboard before putting the original contents back.
    const PASTE_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(120);

    /// Sets the pre-paste pause, in milliseconds, applied before the paste
    /// keystroke is sent (AM-20). This is the plumbing point for
    /// whspr-config's `InjectionSettings::pre_paste_delay_ms`: a consumer
    /// (e.g. the app) reads that setting and calls this once at startup.
    /// Applies to every sink; `0` disables the pause.
    pub fn set_pre_paste_delay_ms(ms: u64) {
        PRE_PASTE_DELAY_MS.store(ms, Ordering::Relaxed);
    }
}

/// The configured pre-paste pause, in milliseconds. Process-global for the
/// same reason as [`last_emitted`]: `EnigoTextSink` is a zero-sized unit
/// struct constructed directly by its callers. Defaults to `0` (no pause).
static PRE_PASTE_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Maps a pre-paste pause in milliseconds to a `Duration` (`0` -> no delay).
/// Pure, so the mapping can be unit-tested without actually sleeping.
fn pre_paste_delay(ms: u64) -> std::time::Duration {
    std::time::Duration::from_millis(ms)
}

/// Whether a separating space should be inserted before `next`, given the
/// trailing character of the previously inserted text.
///
/// True only when the previous text ended in a non-whitespace character and
/// `next` begins with one — so consecutive dictations don't run together
/// ("hello" + "world" -> "hello world") without doubling a space either side
/// already provides. With no previous text, or an empty `next`, no space is
/// added.
fn needs_leading_space(prev_last: Option<char>, next: &str) -> bool {
    match (prev_last, next.chars().next()) {
        (Some(prev), Some(first)) => !prev.is_whitespace() && !first.is_whitespace(),
        _ => false,
    }
}

/// Computes the text `insert` should actually emit for `next` (with a
/// leading space prepended when [`needs_leading_space`] says so), along with
/// the trailing character to remember for the next call. Pure, so the
/// cross-utterance spacing can be exercised without driving enigo or the
/// clipboard.
fn spaced_payload(prev_last: Option<char>, next: &str) -> (String, Option<char>) {
    let payload = if needs_leading_space(prev_last, next) {
        format!(" {next}")
    } else {
        next.to_string()
    };
    // Carry the last char forward; keep the previous one if `next` was empty.
    let new_last = payload.chars().next_back().or(prev_last);
    (payload, new_last)
}

/// The trailing character of the text `EnigoTextSink::insert` last emitted,
/// used to decide whether the next utterance needs a leading space (AM-03).
///
/// Process-global rather than a field on `EnigoTextSink` because the sink is
/// a zero-sized unit struct constructed directly as `EnigoTextSink` by its
/// callers (e.g. the app worker); a single sink drives the focused window in
/// practice.
fn last_emitted() -> &'static Mutex<Option<char>> {
    static LAST: OnceLock<Mutex<Option<char>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

/// Injects text via `paste`, falling back to `type_text` if the paste path
/// fails, so a failed clipboard paste never silently drops the user's
/// dictation. Succeeds if either path succeeds; returns the fallback's error
/// only when both fail. Logs which path delivered the text.
///
/// Split out with injectable closures so the fallback decision is
/// unit-testable without a real display, clipboard, or keystroke synthesis.
fn inject_with_fallback<P, T>(paste: P, type_text: T) -> Result<()>
where
    P: FnOnce() -> Result<()>,
    T: FnOnce() -> Result<()>,
{
    match paste() {
        Ok(()) => {
            tracing::debug!("text injected via clipboard paste");
            Ok(())
        }
        Err(paste_err) => {
            tracing::warn!(
                error = %paste_err,
                "clipboard paste failed; falling back to keystroke typing"
            );
            match type_text() {
                Ok(()) => {
                    tracing::info!("text injected via keystroke fallback");
                    Ok(())
                }
                Err(type_err) => {
                    tracing::error!(
                        error = %type_err,
                        "keystroke fallback also failed; injection lost"
                    );
                    Err(type_err)
                }
            }
        }
    }
}

impl TextSink for EnigoTextSink {
    fn insert(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        // Recover from a poisoned lock rather than propagating a panic — a
        // stale last-char is harmless.
        let mut last = last_emitted().lock().unwrap_or_else(|e| e.into_inner());

        // Keep back-to-back utterances from running together (AM-03).
        let (payload, new_last) = spaced_payload(*last, text);

        // Clipboard paste is the default path: it lands text reliably in
        // editors, terminals, and vim, where raw synthetic keystrokes
        // misbehave (AM-08). If the paste path fails for any reason, degrade
        // to typing the same text rather than dropping the dictation.
        let result = inject_with_fallback(
            || self.paste_from_clipboard(&payload),
            || self.type_text(&payload),
        );

        // Remember what we actually emitted so the next utterance can decide
        // whether it needs a leading space. Only record on success.
        if result.is_ok() {
            *last = new_last;
        }
        result
    }
}

impl EnigoTextSink {
    /// Types text directly using synthetic keystrokes. Public so a caller on
    /// the OS main thread (e.g. the GUI) can type without the clipboard/paste
    /// chord, which on macOS can land as a selection rather than a paste in
    /// some target apps.
    pub fn type_text(&self, text: &str) -> Result<()> {
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
    /// Returns an error — rather than falling back to typing here — if the
    /// clipboard is unavailable, staging fails, or the paste chord fails.
    /// The caller ([`inject_with_fallback`]) owns the fallback to typing, so
    /// the whole degrade-instead-of-drop policy lives in one place.
    fn paste_from_clipboard(&self, text: &str) -> Result<()> {
        let mut clipboard = ArboardClipboard::new()?;

        match stage_and_paste(&mut clipboard, text, || self.send_paste_keystroke()) {
            PasteOutcome::Pasted(result) => result,
            PasteOutcome::Unstaged => Err(WhsprError::Inject(
                "could not stage text on the clipboard".to_string(),
            )),
        }
    }

    /// Simulates the platform paste shortcut (Cmd+V on macOS, Ctrl+V
    /// elsewhere) into whatever window currently has focus.
    fn send_paste_keystroke(&self) -> Result<()> {
        // Pre-paste pause (AM-20): give a slow-to-focus target app a moment
        // before Cmd+V/Ctrl+V lands. Our text is already staged on the
        // clipboard here. Distinct from the post-paste settle further down.
        thread::sleep(pre_paste_delay(PRE_PASTE_DELAY_MS.load(Ordering::Relaxed)));

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| WhsprError::Inject(format!("failed to initialize enigo: {}", e)))?;

        #[cfg(target_os = "macos")]
        self.press_paste_keys(&mut enigo, Key::Meta)?;
        #[cfg(not(target_os = "macos"))]
        self.press_paste_keys(&mut enigo, Key::Control)?;

        // Let the target consume the clipboard before the caller restores it.
        thread::sleep(Self::PASTE_SETTLE_DELAY);

        Ok(())
    }

    /// Presses the modifier key, simulates Cmd+V/Ctrl+V, and releases the
    /// modifier, wrapping each step's error in a consistent error message.
    fn press_paste_keys(&self, enigo: &mut Enigo, modifier: Key) -> Result<()> {
        let press_err = |e| WhsprError::Inject(format!("failed to press modifier key: {}", e));
        let click_err = |e| WhsprError::Inject(format!("failed to click v key: {}", e));
        let release_err = |e| WhsprError::Inject(format!("failed to release modifier key: {}", e));

        enigo.key(modifier, Direction::Press).map_err(press_err)?;
        enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(click_err)?;
        enigo
            .key(modifier, Direction::Release)
            .map_err(release_err)?;

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
    fn pre_paste_delay_maps_millis_to_duration() {
        // 0 ms means no pause.
        assert_eq!(pre_paste_delay(0), Duration::ZERO);
        assert_eq!(pre_paste_delay(150), Duration::from_millis(150));
    }

    #[test]
    fn inject_with_fallback_uses_paste_and_skips_typing_on_success() {
        let typed = std::cell::Cell::new(false);
        let result = inject_with_fallback(
            || Ok(()),
            || {
                typed.set(true);
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert!(
            !typed.get(),
            "typing must not run when the paste path succeeds"
        );
    }

    #[test]
    fn inject_with_fallback_types_when_paste_fails() {
        let typed = std::cell::Cell::new(false);
        let result = inject_with_fallback(
            || Err(WhsprError::Inject("paste failed".into())),
            || {
                typed.set(true);
                Ok(())
            },
        );
        assert!(result.is_ok(), "should recover via the typing fallback");
        assert!(typed.get(), "the typing fallback should have run");
    }

    #[test]
    fn inject_with_fallback_surfaces_error_when_both_fail() {
        let result = inject_with_fallback(
            || Err(WhsprError::Inject("paste failed".into())),
            || Err(WhsprError::Inject("typing failed".into())),
        );
        assert!(result.is_err(), "both paths failing must surface an error");
    }

    #[test]
    fn needs_leading_space_only_between_two_non_whitespace_boundaries() {
        // No previous text: never add a leading space.
        assert!(!needs_leading_space(None, "world"));
        // Two word characters back to back: insert a separating space.
        assert!(needs_leading_space(Some('o'), "world"));
        // Previous text already ended in whitespace: don't double it.
        assert!(!needs_leading_space(Some(' '), "world"));
        assert!(!needs_leading_space(Some('\n'), "world"));
        // Next text already starts with whitespace: don't double it.
        assert!(!needs_leading_space(Some('o'), " world"));
        // Empty next: nothing to separate.
        assert!(!needs_leading_space(Some('o'), ""));
        // Punctuation is non-whitespace, so a following sentence is spaced.
        assert!(needs_leading_space(Some('.'), "Next"));
    }

    #[test]
    fn spaced_payload_separates_consecutive_utterances() {
        // First utterance: no previous text, emitted verbatim.
        let (first, last_after_first) = spaced_payload(None, "hello");
        assert_eq!(first, "hello");
        assert_eq!(last_after_first, Some('o'));

        // Second utterance would otherwise concatenate ("helloworld") — it
        // gets a leading space so the target reads "hello world".
        let (second, last_after_second) = spaced_payload(last_after_first, "world");
        assert_eq!(second, " world");
        assert_eq!(last_after_second, Some('d'));

        // A follow-up that already starts with whitespace isn't doubled.
        let (third, _) = spaced_payload(last_after_second, " again");
        assert_eq!(third, " again");
    }

    #[test]
    fn spaced_payload_carries_previous_char_for_empty_next() {
        // Defensive: an empty follow-up keeps the remembered trailing char.
        assert_eq!(spaced_payload(Some('o'), ""), (String::new(), Some('o')));
    }

    /// `EnigoTextSink::type_text` (the clipboard-unavailable fallback)
    /// synthesizes real keystrokes via enigo into whatever window currently
    /// has OS focus. That needs an active display session and (on macOS)
    /// Accessibility permission granted to the test process, and it types
    /// into whatever happens to be focused when the test runs — not
    /// something to fire unattended in CI. Kept here, `#[ignore]`d, so a
    /// developer with a real desktop session and a scratch text field
    /// focused can run it deliberately via
    /// `cargo test -p whspr-inject -- --ignored`.
    #[test]
    #[ignore = "types real keystrokes into whatever window has OS focus; needs a display + Accessibility permission, run manually"]
    fn type_text_sends_real_keystrokes() {
        let sink = EnigoTextSink;
        sink.type_text("whspr-inject manual keystroke test")
            .expect("type_text should succeed with a display and Accessibility permission granted");
    }

    /// `EnigoTextSink::insert` (the default paste path) saves the current
    /// clipboard, stages the text, simulates a real paste keystroke (Cmd+V /
    /// Ctrl+V) into whatever window has OS focus, then restores the original
    /// clipboard. Same non-hermetic constraints as
    /// `type_text_sends_real_keystrokes` above; the save/restore logic
    /// itself is covered hermetically by the `stage_and_paste_*` tests.
    #[test]
    #[ignore = "pastes into whatever window has OS focus; needs a display + Accessibility permission, run manually"]
    fn paste_from_clipboard_sends_real_paste_keystroke() {
        let sink = EnigoTextSink;
        sink.insert("whspr-inject manual paste test")
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

    /// D-13: dropping a `GlobalHotkeyListener` must release the OS-level
    /// hotkey, so the exact same combo can be registered again afterward. If
    /// `Drop` didn't unregister, the second registration would fail because
    /// the combo is still held by the OS.
    ///
    /// Registering a global hotkey needs a live OS hotkey manager (a display
    /// session, and on some platforms specific permissions), so this can't
    /// run hermetically — it's `#[ignore]`d for CI and run manually with
    /// `cargo test -p whspr-inject -- --ignored`. It also skips cleanly if
    /// the environment can't register a hotkey at all (an environment
    /// limitation, not a D-13 failure).
    #[test]
    #[ignore = "needs a live OS hotkey manager (display/permissions); run manually"]
    fn dropping_listener_frees_the_hotkey_for_reregistration() {
        let first = match GlobalHotkeyListener::new() {
            Ok(listener) => listener,
            Err(e) => {
                eprintln!("skipping: no hotkey manager available in this environment: {e}");
                return;
            }
        };
        // Dropping runs the `Drop` impl, which unregisters the combo.
        drop(first);

        // Registering the same combo again only succeeds if it was freed.
        GlobalHotkeyListener::new().expect(
            "re-registering the hotkey after drop should succeed once Drop has released it",
        );
    }
}
