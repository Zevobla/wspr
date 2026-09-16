//! Frontmost-app detection for "style by window" (J-04): feeds
//! `whspr_core::RefineContext.app_name`, so a refiner backend can format
//! text differently depending on what app it's being dictated into (see
//! `whspr_refine`'s cleanup prompt, which adds "this text is going into
//! <app>" whenever `app_name` is set). `crate::worker` reads
//! `frontmost_app_name()` at the moment a dictation capture starts and
//! threads the result into the pipeline's `RefineContext` via
//! `app_name_for`.
//!
//! ## Privacy
//! Only the frontmost app's localized (human-readable) name is ever read --
//! never its bundle identifier, window title, or contents -- and only when
//! the user's `[device].active_window` setting is turned on (see
//! `app_name_for`). The name is never persisted anywhere except the
//! existing per-turn `RefineContext` it's already threaded into for that one
//! dictation.
//!
//! ## Platform support
//! Implemented for macOS only, via `objc2-app-kit`'s safe `NSWorkspace`
//! wrapper (`sharedWorkspace().frontmostApplication()?.localizedName()`).
//! Not implemented on Windows/Linux yet -- both return `None` -- see
//! `crate::tray`'s module doc comment for the tone on documented platform
//! gaps like this one; bridging this to the Win32 foreground-window API or
//! an X11/Wayland equivalent is a separate platform integration, not
//! attempted here.

/// The frontmost (focused) application's localized name, if any. Never
/// panics: absence at any step (no frontmost app, no localized name)
/// degrades to `None` rather than propagating an error, since this is
/// context for text cleanup, not something dictation should ever hard-fail
/// over.
#[cfg(target_os = "macos")]
pub fn frontmost_app_name() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(app.localizedName()?.to_string())
}

/// Not implemented on Windows/Linux yet -- always `None`. See the module
/// doc comment's "Platform support" section.
#[cfg(not(target_os = "macos"))]
pub fn frontmost_app_name() -> Option<String> {
    None
}

/// Whether to attach the frontmost app's name to a dictation's
/// `RefineContext`, per the user's `[device].active_window` setting.
/// Returns `None` whenever the setting is off, regardless of what was
/// detected -- pulled out as a pure fn so this on/off decision is
/// unit-testable without a real `NSWorkspace` call.
pub(crate) fn app_name_for(enabled: bool, detected: Option<String>) -> Option<String> {
    if enabled {
        detected
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_for_enabled_with_detected_name_returns_it() {
        assert_eq!(
            app_name_for(true, Some("Visual Studio Code".to_string())),
            Some("Visual Studio Code".to_string())
        );
    }
}
