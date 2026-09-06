//! Foreground-app detection for per-app refine styling (J-04): populates
//! `RefineContext.app_name`, consumed by `whspr_refine::build_cleanup_prompt`.
//!
//! Per-platform, using only permissive-licensed crates -- this feature's
//! first attempt used `active-win-pos-rs`, dropped because its Linux
//! backend unconditionally pulled in `hyprland`/`hyprland-macros`
//! (GPL-3.0-or-later, no permissive alternative), which broke this
//! project's copyleft-free dependency graph. Every platform here degrades
//! to `None` on any failure (no active window, missing permissions, a
//! Wayland session with no X11 available, ...) -- per-app styling is a
//! nice-to-have, never a reason to fail a dictation turn.

#[cfg(target_os = "macos")]
mod platform {
    use objc2_app_kit::NSWorkspace;

    /// The frontmost app's localized name, via `NSWorkspace`. `objc2`'s
    /// bindings for these particular methods don't require a
    /// `MainThreadMarker` (unlike window/view-mutating AppKit APIs), so
    /// this is safe to call from the worker's background task, not just
    /// iced's own winit thread.
    pub fn frontmost_app_name() -> Option<String> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        Some(app.localizedName()?.to_string())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    /// The foreground window's owning process's module (.exe) base name:
    /// `GetForegroundWindow` -> `GetWindowThreadProcessId` -> `OpenProcess`
    /// -> `GetModuleBaseNameW`.
    pub fn frontmost_app_name() -> Option<String> {
        // SAFETY: every Win32 call below is a read-only query against a
        // window/process the OS already reports as existing; every return
        // value (a null handle, a zero pid, a zero length) is checked
        // before use, and the opened process handle is closed on every
        // exit path.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }

            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }

            const MAX_PATH: usize = 260;
            let mut buffer = [0u16; MAX_PATH];
            let len = GetModuleBaseNameW(
                process,
                std::ptr::null_mut(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            );
            CloseHandle(process);

            if len == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buffer[..len as usize]))
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use x11rb::connection::Connection;
    use x11rb::properties::WmClass;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    /// The active window's `WM_CLASS` "class" field (typically the
    /// application's identifier, e.g. "firefox", "code"), via the
    /// `_NET_ACTIVE_WINDOW` root-window property. X11 only: there's no
    /// equivalent standard protocol on Wayland, so connecting to an X
    /// server at all (including "there isn't one, this is Wayland") is
    /// itself just another failure mode that degrades to `None`.
    pub fn frontmost_app_name() -> Option<String> {
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen_num)?.root;

        let net_active_window = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;

        let reply = conn
            .get_property(false, root, net_active_window, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let window = reply.value32()?.next()?;
        if window == 0 {
            return None;
        }

        let class_reply = WmClass::get(&conn, window).ok()?.reply().ok()??;
        let class = class_reply.class();
        if class.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(class).into_owned())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    /// Not implemented on this platform.
    pub fn frontmost_app_name() -> Option<String> {
        None
    }
}

pub use platform::frontmost_app_name;

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors `crate::devices`' identically-reasoned
    /// `list_input_device_names_does_not_panic`: we can't assert on the
    /// actual active window/app in a sandboxed/headless CI environment
    /// (there may be none, or no X server on Linux), only that querying
    /// it is safe to call.
    #[test]
    fn frontmost_app_name_does_not_panic() {
        let _ = frontmost_app_name();
    }
}
