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
