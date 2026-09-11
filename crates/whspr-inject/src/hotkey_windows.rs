//! Windows global-hotkey listener and its Win32 message-pump FFI.
//!
//! On Windows, `GlobalHotKeyManager` owns a hidden, thread-affine Win32
//! window and is `!Send + !Sync`, so — unlike the other platforms — it can't
//! be stored in a field of the `Send + Sync` listener. It's created, used,
//! and dropped entirely on a dedicated pump thread; the struct keeps only the
//! `Send + Sync` handles needed to stop that thread. See the non-Windows
//! counterpart in `hotkey_unix`.

use std::thread;

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::GlobalHotKeyManager;

use whspr_core::{Result, WhsprError};

use crate::default_hotkey_modifiers;

/// Listens for the configured global hotkey on Windows.
///
/// On Windows, `GlobalHotKeyManager` owns a hidden, *thread-affine* Win32
/// window: `RegisterHotKey` targets that window, and its `WM_HOTKEY`
/// notifications are only delivered while a message loop pumps messages on
/// the exact thread that created it. The manager is therefore `!Send +
/// !Sync`, so — unlike macOS — it can't be stored in a field of this
/// `Send + Sync` listener. Instead it's created, used, and dropped entirely
/// on a dedicated pump thread, and this struct keeps only the `Send + Sync`
/// handles needed to stop that thread. Events still reach
/// [`subscribe`](whspr_core::HotkeyListener::subscribe) via the process-global
/// `GlobalHotKeyEvent::receiver()`, exactly as on macOS.
pub struct GlobalHotkeyListener {
    /// The pump thread's id, so `Drop` can post `WM_QUIT` to stop its loop.
    pump_thread_id: u32,
    /// The pump thread, joined on drop once it has unregistered the hotkey
    /// and destroyed the manager's window on its own thread. `Option` so
    /// `Drop` can take ownership to join it.
    pump: Option<thread::JoinHandle<()>>,
}

impl GlobalHotkeyListener {
    /// Creates a new global hotkey listener with the platform default hotkey
    /// (`Ctrl+Shift+Space` on Windows).
    ///
    /// The manager and its message loop live on a dedicated thread; this
    /// blocks until that thread reports whether creation + registration
    /// succeeded, so a failure surfaces from `new()` synchronously, exactly
    /// like the non-Windows path.
    pub fn new() -> Result<Self> {
        // Carries the pump thread's id back on success, or the setup error on
        // failure, so `new()` mirrors the synchronous contract of the other
        // platforms.
        let (setup_tx, setup_rx) = std::sync::mpsc::channel::<Result<u32>>();

        let pump = thread::spawn(move || {
            // Captured up front so `Drop` can target this exact thread with
            // `PostThreadMessageW(WM_QUIT)`.
            let thread_id = unsafe { winapi::GetCurrentThreadId() };

            // Everything from here is thread-affine and never leaves this
            // thread: the manager owns a Win32 window that must be created,
            // used, and destroyed all on the same thread.
            let manager = match GlobalHotKeyManager::new() {
                Ok(manager) => manager,
                Err(e) => {
                    let _ = setup_tx.send(Err(WhsprError::Inject(format!(
                        "failed to create global hotkey manager: {}",
                        e
                    ))));
                    return;
                }
            };

            let hotkey = HotKey::new(Some(default_hotkey_modifiers()), Code::Space);

            if let Err(e) = manager.register(hotkey) {
                let _ = setup_tx.send(Err(WhsprError::Inject(format!(
                    "failed to register global hotkey: {}",
                    e
                ))));
                return;
            }

            // Registration succeeded: hand the thread id back so the listener
            // can stop us later. If the receiver is already gone (the listener
            // was dropped mid-setup), unregister and let `manager` drop here,
            // on its creating thread.
            if setup_tx.send(Ok(thread_id)).is_err() {
                let _ = manager.unregister(hotkey);
                return;
            }

            // Pump messages so `RegisterHotKey`'s `WM_HOTKEY` notifications are
            // dispatched to the manager's window proc, which forwards them to
            // `GlobalHotKeyEvent`. Returns once `Drop` posts `WM_QUIT`.
            run_message_loop();

            // Release the hotkey, then let `manager` drop here so its
            // `DestroyWindow` runs on the same thread that created the window.
            let _ = manager.unregister(hotkey);
        });

        match setup_rx.recv() {
            Ok(Ok(pump_thread_id)) => Ok(GlobalHotkeyListener {
                pump_thread_id,
                pump: Some(pump),
            }),
            Ok(Err(e)) => {
                // Setup failed on the pump thread; it has already returned, so
                // join it (best-effort) and surface the error.
                let _ = pump.join();
                Err(e)
            }
            Err(_) => {
                // The pump thread exited before reporting readiness (e.g. it
                // panicked). Join it so we don't leak the handle, then report.
                let _ = pump.join();
                Err(WhsprError::Inject(
                    "global hotkey setup thread exited before reporting readiness".to_string(),
                ))
            }
        }
    }
}

impl Default for GlobalHotkeyListener {
    fn default() -> Self {
        Self::new().expect("failed to initialize GlobalHotkeyListener")
    }
}

impl Drop for GlobalHotkeyListener {
    /// Stops the pump thread so it unregisters the hotkey and destroys the
    /// manager's window on its own thread (D-13). Best-effort: `Drop` must
    /// never panic, so a failed post or join is ignored.
    fn drop(&mut self) {
        // `WM_QUIT` makes the pump thread's `GetMessageW` return 0, breaking
        // its loop; it then unregisters the hotkey and drops the manager
        // (destroying the window) on that same thread.
        unsafe {
            winapi::PostThreadMessageW(self.pump_thread_id, winapi::WM_QUIT, 0, 0);
        }
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
    }
}

/// Minimal Win32 FFI for the hotkey pump thread's message loop.
///
/// `windows-sys` isn't a workspace dependency (and per CLAUDE.md this crate
/// must not add one), so the handful of `user32`/`kernel32` entry points the
/// pump needs are declared directly here rather than pulling in a new crate.
/// Signatures mirror the Win32 headers exactly: `HWND` is an opaque pointer,
/// `WPARAM`/`LPARAM`/`LRESULT` are pointer-sized, `BOOL` is a 32-bit int, and
/// `MSG` is a `#[repr(C)]` struct the OS writes into.
mod winapi {
    #![allow(non_snake_case, dead_code)]

    use std::ffi::c_void;

    /// Win32 `HWND`: an opaque window handle (a pointer).
    pub type Hwnd = *mut c_void;
    /// Win32 `WPARAM`: pointer-sized unsigned message parameter.
    pub type Wparam = usize;
    /// Win32 `LPARAM`: pointer-sized signed message parameter.
    pub type Lparam = isize;
    /// Win32 `LRESULT`: pointer-sized signed message result.
    pub type Lresult = isize;
    /// Win32 `BOOL`: a 32-bit integer (`0` = false).
    pub type Bool = i32;

    /// Win32 `POINT`, embedded in [`Msg`].
    #[repr(C)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    /// Win32 `MSG`, filled in by [`GetMessageW`]. The layout must match the
    /// header exactly, since the OS writes through the pointer we hand it.
    #[repr(C)]
    pub struct Msg {
        pub hwnd: Hwnd,
        pub message: u32,
        pub wParam: Wparam,
        pub lParam: Lparam,
        pub time: u32,
        pub pt: Point,
    }

    /// `WM_QUIT`: posted to the pump thread to make [`GetMessageW`] return 0
    /// and end the message loop.
    pub const WM_QUIT: u32 = 0x0012;

    #[link(name = "user32")]
    extern "system" {
        /// Blocks until a message is available for this thread, then removes
        /// it from the queue and writes it into `lpMsg`. Returns 0 on
        /// `WM_QUIT`, -1 on error, nonzero otherwise.
        pub fn GetMessageW(
            lpMsg: *mut Msg,
            hWnd: Hwnd,
            wMsgFilterMin: u32,
            wMsgFilterMax: u32,
        ) -> Bool;
        /// Translates virtual-key messages into character messages. A no-op
        /// for `WM_HOTKEY`, but part of the canonical pump.
        pub fn TranslateMessage(lpMsg: *const Msg) -> Bool;
        /// Dispatches a message to its window procedure — this is what runs
        /// the hotkey manager's window proc for a `WM_HOTKEY`.
        pub fn DispatchMessageW(lpMsg: *const Msg) -> Lresult;
        /// Posts a message to the message queue of the thread `idThread`.
        /// Used to deliver `WM_QUIT` to the pump thread on shutdown.
        pub fn PostThreadMessageW(idThread: u32, msg: u32, wParam: Wparam, lParam: Lparam) -> Bool;
    }

    #[link(name = "kernel32")]
    extern "system" {
        /// Returns the calling thread's id, captured on the pump thread so
        /// the listener can target it with [`PostThreadMessageW`].
        pub fn GetCurrentThreadId() -> u32;
    }
}

/// Runs a standard Win32 message loop until `WM_QUIT`, dispatching each
/// message so the hotkey manager's window procedure runs (that proc is what
/// turns a `WM_HOTKEY` into a `GlobalHotKeyEvent`). Must be called on the
/// same thread that created the manager's window.
fn run_message_loop() {
    // A zeroed `MSG` is a valid empty buffer for the OS to fill in. Passing a
    // null `hWnd` filter makes `GetMessageW` return every message for this
    // thread — both the window's `WM_HOTKEY` and the thread-targeted
    // `WM_QUIT` used to stop the loop.
    let mut msg: winapi::Msg = unsafe { std::mem::zeroed() };
    loop {
        // Blocks until a message arrives; this thread does nothing else, so
        // blocking (rather than spinning) is exactly what we want.
        let ret = unsafe { winapi::GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };

        // `GetMessageW` returns 0 on `WM_QUIT` and -1 on error; either way
        // there is nothing left to pump, so stop.
        if ret <= 0 {
            break;
        }

        // SAFETY: `msg` was just populated by a successful `GetMessageW`.
        unsafe {
            winapi::TranslateMessage(&msg);
            winapi::DispatchMessageW(&msg);
        }
    }
}
