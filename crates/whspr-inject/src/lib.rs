//! Global hotkey listening and text injection. Implements
//! `whspr_core::HotkeyListener` and `whspr_core::TextSink`.

use global_hotkey::hotkey::Modifiers;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::thread;
use tokio::sync::mpsc;

use whspr_core::{HotkeyEvent, HotkeyListener};

mod clipboard;
mod debounce;
mod hotkey_config;
mod text_sink;

// The OS-level hotkey listener has one file per platform: off Windows the
// manager is `Send + Sync` and owned directly; on Windows it's `!Send +
// !Sync` and driven from a message-pump thread. Each file defines its own
// `GlobalHotkeyListener`, re-exported here so the rest of the crate (and the
// un-gated `impl HotkeyListener` below) sees a single type.
#[cfg(not(windows))]
mod hotkey_unix;
#[cfg(not(windows))]
pub use hotkey_unix::GlobalHotkeyListener;

#[cfg(windows)]
mod hotkey_windows;
#[cfg(windows)]
pub use hotkey_windows::GlobalHotkeyListener;

pub use debounce::{DebounceAction, DebouncedHotkeyListener, HotkeyDebouncer};
pub use hotkey_config::hotkey_supported;
pub use text_sink::EnigoTextSink;

/// The fresh-install default global-hotkey modifiers.
///
/// On Windows `Ctrl+Space` collides with the IME language toggle, so the
/// Windows default is `Ctrl+Shift+Space`; macOS and Linux keep the plain
/// `Ctrl+Space`. The key stays [`Code::Space`] on every platform. This is
/// only the default a fresh install starts with -- a persisted user hotkey
/// still overrides it.
///
/// Written as a `cfg!` expression rather than a `#[cfg]` const pair so both
/// arms are type-checked on every host (the Windows arm compiles on macOS
/// too), then constant-folded to the host's value at build time.
pub(crate) fn default_hotkey_modifiers() -> Modifiers {
    if cfg!(target_os = "windows") {
        Modifiers::CONTROL | Modifiers::SHIFT
    } else {
        Modifiers::CONTROL
    }
}

/// Display label of the platform default hotkey (`"Ctrl+Space"`, or
/// `"Ctrl+Shift+Space"` on Windows) — the single source of truth for UI hints
/// so the label can never drift from what [`default_hotkey_modifiers`] and
/// [`Code::Space`] actually register.
///
/// The Windows arm adds `Shift` because `Ctrl+Space` collides with the IME
/// language toggle; macOS and Linux keep the plain `Ctrl+Space`. Written as a
/// `cfg!` expression (not a `#[cfg]` pair) so both arms are type-checked on
/// every host, then constant-folded to the host's value at build time.
pub const fn default_hotkey_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Ctrl+Shift+Space"
    } else {
        "Ctrl+Space"
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

/// Spawns the background thread that forwards process-global hotkey events
/// onto a fresh channel, returning its receiving end.
///
/// Shared by every platform's [`HotkeyListener`] impl: events are read from
/// the process-global [`GlobalHotKeyEvent::receiver()`] regardless of how (or
/// on which thread) the underlying hotkey was registered, so the forwarding
/// logic doesn't depend on the platform's registration strategy.
fn subscribe_global_events() -> mpsc::Receiver<HotkeyEvent> {
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

impl HotkeyListener for GlobalHotkeyListener {
    fn subscribe(&self) -> mpsc::Receiver<HotkeyEvent> {
        subscribe_global_events()
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
    fn default_hotkey_modifiers_are_platform_specific() {
        let mods = default_hotkey_modifiers();
        // Windows defaults to Ctrl+Shift+Space (Ctrl+Space collides with the
        // IME language toggle); macOS/Linux keep plain Ctrl+Space.
        #[cfg(target_os = "windows")]
        assert_eq!(mods, Modifiers::CONTROL | Modifiers::SHIFT);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(mods, Modifiers::CONTROL);
    }

    #[test]
    fn default_hotkey_label_matches_platform_modifiers() {
        // The UI label must stay in lockstep with the modifiers actually
        // registered: Windows adds Shift, everyone else is plain Ctrl+Space.
        #[cfg(target_os = "windows")]
        assert_eq!(default_hotkey_label(), "Ctrl+Shift+Space");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(default_hotkey_label(), "Ctrl+Space");
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
