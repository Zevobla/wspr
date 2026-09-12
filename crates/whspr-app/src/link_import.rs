//! The "Add from a link" import dialog: its dialog state and the `update`
//! handler that drives resolve + the user's import choices. The modal view
//! lives in `crate::hub::link_import`; this module owns the `LinkImport`
//! state that view renders (re-exported through `crate::state`) and the
//! messages that mutate it.
//!
//! Handlers are chained through `crate::app::update`'s catch-all -- the same
//! shape as `crate::note_desk::update` -- so `app.rs` stays under its line
//! cap (AA-06) and carries no link-import arms inline. `LinkImportResolve`
//! shells out to `whspr_import::resolve` off the UI thread via
//! `Task::perform`; the other messages only mutate the dialog struct.

use iced::Task;

use crate::state::{Message, State};

/// All state for the "Add from a link" modal dialog. `None` on `State` means
/// the dialog is closed; `Some(..)` opens it. Seeded empty by
/// [`LinkImport::new`]; `LinkImportResolved` fills `media` and seeds the
/// per-chapter include flags and the captions/transcribe default.
#[derive(Debug, Default)]
pub struct LinkImport {
    /// Live contents of the URL text input.
    pub url: String,
    /// Whether a `resolve` call is in flight (disables Resolve, shows a
    /// pending label).
    pub resolving: bool,
    /// The resolved media metadata once `resolve` succeeds; `None` before.
    pub media: Option<whspr_import::MediaInfo>,
    /// The last resolve error, shown in the dialog; `None` when there's none.
    pub error: Option<String>,
    /// Import published captions (`true`) or transcribe locally (`false`).
    /// Defaulted on resolve: captions when a human track exists.
    pub use_captions: bool,
    /// The caption language code the captions path would use, if any.
    pub caption_lang: Option<String>,
    /// Per-chapter "include as a note heading" flags, parallel to
    /// `media.chapters`. Seeded all-true on resolve.
    pub chapters_included: Vec<bool>,
    /// Live contents of the "clip from" `MM:SS` input.
    pub clip_start: String,
    /// Live contents of the "clip to" `MM:SS` input.
    pub clip_end: String,
    /// The browser to borrow sign-in cookies from, once chosen (e.g.
    /// `"safari"`); `None` runs anonymously.
    pub cookies_browser: Option<String>,
}

impl LinkImport {
    /// A freshly opened, empty dialog.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Handles the link-import messages, returning `Some(task)` when it owns the
/// message and `None` otherwise so `crate::app::update`'s catch-all keeps
/// forwarding to the other handlers (mirrors `crate::note_desk::update`).
pub fn update(state: &mut State, message: &Message) -> Option<Task<Message>> {
    match message {
        Message::LinkImportOpen => {
            state.link_import = Some(LinkImport::new());
            Some(Task::none())
        }
        Message::LinkImportCancel => {
            state.link_import = None;
            Some(Task::none())
        }
        Message::LinkImportUrl(url) => {
            if let Some(li) = state.link_import.as_mut() {
                li.url = url.clone();
            }
            Some(Task::none())
        }
        Message::LinkImportResolve => Some(start_resolve(state)),
        Message::LinkImportResolved(result) => {
            apply_resolved(state, result);
            Some(Task::none())
        }
        Message::LinkImportUseCaptions(use_captions) => {
            if let Some(li) = state.link_import.as_mut() {
                li.use_captions = *use_captions;
            }
            Some(Task::none())
        }
        Message::LinkImportToggleChapter(index) => {
            if let Some(li) = state.link_import.as_mut() {
                if let Some(flag) = li.chapters_included.get_mut(*index) {
                    *flag = !*flag;
                }
            }
            Some(Task::none())
        }
        Message::LinkImportClipStart(value) => {
            if let Some(li) = state.link_import.as_mut() {
                li.clip_start = value.clone();
            }
            Some(Task::none())
        }
        Message::LinkImportClipEnd(value) => {
            if let Some(li) = state.link_import.as_mut() {
                li.clip_end = value.clone();
            }
            Some(Task::none())
        }
        Message::LinkImportBorrowCookies(browser) => {
            if let Some(li) = state.link_import.as_mut() {
                li.cookies_browser = Some(browser.clone());
            }
            Some(Task::none())
        }
        Message::LinkImportConfirm => {
            // TODO(F3): run captions/transcribe -> build NoteDeskState -> EnterNoteDesk
            state.link_import = None;
            state.transcribe_status =
                Some("Opening note desk\u{2026} (wiring lands next)".to_string());
            Some(Task::none())
        }
        _ => None,
    }
}

/// Kicks off `whspr_import::resolve` for the current URL off the UI thread,
/// marking the dialog `resolving`. A blank URL is a no-op. yt-dlp's error is
/// mapped to `String` so it lands in `LinkImportResolved(Err(..))`.
fn start_resolve(state: &mut State) -> Task<Message> {
    let Some(li) = state.link_import.as_mut() else {
        return Task::none();
    };
    let url = li.url.trim().to_string();
    if url.is_empty() {
        return Task::none();
    }
    li.resolving = true;
    li.error = None;
    let cookies = match &li.cookies_browser {
        Some(browser) => whspr_import::CookiesFrom::Browser(browser.clone()),
        None => whspr_import::CookiesFrom::None,
    };
    Task::perform(
        async move {
            whspr_import::resolve(&url, cookies)
                .await
                .map_err(|e| e.to_string())
        },
        Message::LinkImportResolved,
    )
}

/// Folds a finished `resolve` into the dialog: on success stores the media,
/// seeds every chapter as included, and defaults the import path to captions
/// when a human track exists (else transcribe, pre-selecting an auto track's
/// language if any). On failure records the error and clears any media.
fn apply_resolved(state: &mut State, result: &Result<whspr_import::MediaInfo, String>) {
    let Some(li) = state.link_import.as_mut() else {
        return;
    };
    li.resolving = false;
    match result {
        Ok(media) => {
            li.chapters_included = vec![true; media.chapters.len()];
            if let Some(lang) = media.human_captions.first() {
                li.use_captions = true;
                li.caption_lang = Some(lang.code.clone());
            } else {
                li.use_captions = false;
                li.caption_lang = media.auto_captions.first().map(|l| l.code.clone());
            }
            li.error = None;
            li.media = Some(media.clone());
        }
        Err(error) => {
            li.error = Some(error.clone());
            li.media = None;
        }
    }
}
