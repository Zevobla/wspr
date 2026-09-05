//! A small clipboard abstraction and the save/stage/restore sequence used
//! by the clipboard-paste injection path.
//!
//! Hiding the real `arboard` behind the [`Clipboard`] trait lets the
//! save/set/restore logic in [`stage_and_paste`] be unit-tested with an
//! in-memory fake, without a display server or a live system clipboard.

use whspr_core::{Result, WhsprError};

/// A minimal system-clipboard abstraction.
///
/// The clipboard-paste path needs to *save* the user's current clipboard,
/// stage our own text, paste, then *restore* what was there before. This
/// trait is the seam that makes that sequence testable.
pub(crate) trait Clipboard {
    /// Returns the current clipboard text, or an error if there is no text
    /// on the clipboard (e.g. it's empty or holds a non-text payload).
    fn get_text(&mut self) -> Result<String>;
    /// Replaces the clipboard contents with `text`.
    fn set_text(&mut self, text: &str) -> Result<()>;
    /// Empties the clipboard.
    fn clear(&mut self) -> Result<()>;
}

/// The real [`Clipboard`], backed by the system clipboard via `arboard`.
pub(crate) struct ArboardClipboard(arboard::Clipboard);

impl ArboardClipboard {
    /// Opens a handle to the system clipboard.
    pub(crate) fn new() -> Result<Self> {
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
pub(crate) enum PasteOutcome {
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
pub(crate) fn stage_and_paste<C, F>(clipboard: &mut C, text: &str, paste: F) -> PasteOutcome
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A mid-paste error must not strand our text on the clipboard: the
    /// original is still restored, and the paste error is reported.
    #[test]
    fn stage_and_paste_restores_original_when_paste_errors() {
        let mut clipboard = MockClipboard::with_content(Some("user's data"));

        let outcome = stage_and_paste(&mut clipboard, "injected text", || {
            Err(WhsprError::Inject("paste failed".into()))
        });

        assert!(matches!(outcome, PasteOutcome::Pasted(Err(_))));
        assert_eq!(clipboard.content.as_deref(), Some("user's data"));
    }

    /// Even if the paste step *panics*, the guard's `Drop` still runs while
    /// unwinding, so the user's clipboard is restored rather than left
    /// holding our injected text.
    #[test]
    fn stage_and_paste_restores_original_when_paste_panics() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut clipboard = MockClipboard::with_content(Some("user's data"));

        // Silence the default panic hook so the deliberate panic below
        // doesn't clutter the test output.
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = stage_and_paste(&mut clipboard, "injected text", || panic!("paste blew up"));
        }));
        std::panic::set_hook(prev_hook);

        assert!(result.is_err(), "the paste panic should propagate");
        assert_eq!(clipboard.content.as_deref(), Some("user's data"));
    }

    /// When our text can't even be staged (clipboard access denied), the
    /// sequence reports `Unstaged` without running the paste and leaves the
    /// clipboard untouched — signalling the caller to fall back to typing.
    #[test]
    fn stage_and_paste_reports_unstaged_when_set_text_fails() {
        let mut clipboard = MockClipboard::with_content(Some("user's data"));
        clipboard.set_fails = true;

        let outcome = stage_and_paste(&mut clipboard, "injected text", || {
            panic!("paste must not run when staging failed")
        });

        assert!(matches!(outcome, PasteOutcome::Unstaged));
        // The clipboard was never modified.
        assert_eq!(clipboard.content.as_deref(), Some("user's data"));
        assert!(clipboard.writes.is_empty());
    }

    /// If the clipboard started empty (nothing to save), restoring means
    /// clearing it, so our injected text isn't left behind.
    #[test]
    fn stage_and_paste_clears_clipboard_that_started_empty() {
        let mut clipboard = MockClipboard::with_content(None);

        let outcome = stage_and_paste(&mut clipboard, "injected text", || Ok(()));

        assert!(matches!(outcome, PasteOutcome::Pasted(Ok(()))));
        assert_eq!(clipboard.content, None);
    }

    /// Verifies the real [`ArboardClipboard`] seam is genuine: text set
    /// through it actually round-trips through the system clipboard,
    /// including non-ASCII text (the case the paste path exists for). This
    /// exercises the same trait impl the save/restore path uses, but only
    /// the clipboard — not the focused window — so it needs no synthesized
    /// keystrokes.
    ///
    /// Side effect: this overwrites the real system clipboard. We save and
    /// best-effort restore whatever was there before.
    #[test]
    fn clipboard_round_trip_preserves_unicode_text() {
        let mut clipboard = match ArboardClipboard::new() {
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

        // Restore, or clear if the clipboard was empty before.
        match previous {
            Some(previous) => {
                let _ = clipboard.set_text(&previous);
            }
            None => {
                let _ = clipboard.clear();
            }
        }
    }
}
