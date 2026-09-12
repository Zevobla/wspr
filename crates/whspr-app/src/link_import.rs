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

use whspr_core::AsrBackend;

use crate::note_desk::{NoteDeskState, NoteHeading};
use crate::state::{Message, State};

/// The payload of a finished link import: the note-desk title, the kept chapter
/// headings, and the transcript (from published captions or a local
/// transcribe). Boxed in [`Message::LinkImportImported`] to keep the `Message`
/// enum small (`clippy::result_large_err`), mirroring `LinkImportResolved`.
pub type ImportedNote = (String, Vec<NoteHeading>, whspr_core::Transcript);

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
                .map(Box::new)
                .map_err(|e| e.to_string())
        },
        Message::LinkImportResolved,
    )
}

/// Folds a finished `resolve` into the dialog: on success stores the media,
/// seeds every chapter as included, and defaults the import path to captions
/// when a human track exists (else transcribe, pre-selecting an auto track's
/// language if any). On failure records the error and clears any media.
fn apply_resolved(state: &mut State, result: &Result<Box<whspr_import::MediaInfo>, String>) {
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
            li.media = Some(media.as_ref().clone());
        }
        Err(error) => {
            li.error = Some(error.clone());
            li.media = None;
        }
    }
}

/// Parses an `MM:SS` (or bare-seconds) clip field into seconds. Blank or
/// unparseable input is `None`, so an empty field just means "no bound".
fn parse_mmss(value: &str) -> Option<f32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.split_once(':') {
        Some((mins, secs)) => {
            let mins: f32 = mins.trim().parse().ok()?;
            let secs: f32 = secs.trim().parse().ok()?;
            Some(mins * 60.0 + secs)
        }
        None => trimmed.parse().ok(),
    }
}

/// A [`whspr_import::ClipRange`] from the dialog's clip inputs, but only when
/// both bounds parse and describe a non-empty forward range; otherwise the
/// whole item is imported (`None`).
fn parse_clip_range(start: &str, end: &str) -> Option<whspr_import::ClipRange> {
    let start = parse_mmss(start)?;
    let end = parse_mmss(end)?;
    (end > start).then(|| whspr_import::ClipRange::new(start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_config::Config;
    use whspr_import::{Chapter, Lang, MediaInfo};

    fn open_state() -> State {
        let mut state = State::new(Config::default());
        assert!(update(&mut state, &Message::LinkImportOpen).is_some());
        state
    }

    fn sample_media(human: bool) -> MediaInfo {
        MediaInfo {
            title: "Lecture".to_string(),
            chapters: vec![
                Chapter {
                    title: "One".to_string(),
                    start_secs: 0.0,
                    end_secs: 60.0,
                },
                Chapter {
                    title: "Two".to_string(),
                    start_secs: 60.0,
                    end_secs: 120.0,
                },
            ],
            human_captions: if human {
                vec![Lang {
                    code: "en".to_string(),
                    name: Some("English".to_string()),
                }]
            } else {
                Vec::new()
            },
            auto_captions: vec![Lang {
                code: "de".to_string(),
                name: None,
            }],
            ..MediaInfo::default()
        }
    }

    #[test]
    fn open_seeds_a_dialog_and_cancel_clears_it() {
        let mut state = open_state();
        assert!(state.link_import.is_some());
        assert!(update(&mut state, &Message::LinkImportCancel).is_some());
        assert!(state.link_import.is_none());
    }

    #[test]
    fn url_edits_are_stored() {
        let mut state = open_state();
        assert!(update(&mut state, &Message::LinkImportUrl("https://x".to_string())).is_some());
        assert_eq!(state.link_import.as_ref().unwrap().url, "https://x");
    }

    #[test]
    fn resolved_seeds_all_chapters_included() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
        )
        .is_some());
        let li = state.link_import.as_ref().unwrap();
        assert_eq!(li.chapters_included, vec![true, true]);
        assert!(li.media.is_some());
        assert!(!li.resolving);
    }

    #[test]
    fn resolved_defaults_to_captions_when_a_human_track_exists() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
        )
        .is_some());
        let li = state.link_import.as_ref().unwrap();
        assert!(li.use_captions);
        assert_eq!(li.caption_lang.as_deref(), Some("en"));
    }

    #[test]
    fn resolved_defaults_to_transcribe_without_a_human_track() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Ok(Box::new(sample_media(false))))
        )
        .is_some());
        let li = state.link_import.as_ref().unwrap();
        assert!(!li.use_captions);
        assert_eq!(li.caption_lang.as_deref(), Some("de"));
    }

    #[test]
    fn resolved_error_is_recorded() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Err("boom".to_string()))
        )
        .is_some());
        let li = state.link_import.as_ref().unwrap();
        assert_eq!(li.error.as_deref(), Some("boom"));
        assert!(li.media.is_none());
    }

    #[test]
    fn toggle_chapter_flips_one_flag() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
        )
        .is_some());
        assert!(update(&mut state, &Message::LinkImportToggleChapter(0)).is_some());
        assert_eq!(
            state.link_import.as_ref().unwrap().chapters_included,
            vec![false, true]
        );
    }

    #[test]
    fn borrow_cookies_records_the_browser() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportBorrowCookies("safari".to_string())
        )
        .is_some());
        assert_eq!(
            state
                .link_import
                .as_ref()
                .unwrap()
                .cookies_browser
                .as_deref(),
            Some("safari")
        );
    }

    #[test]
    fn confirm_closes_the_dialog_and_sets_status() {
        let mut state = open_state();
        assert!(update(
            &mut state,
            &Message::LinkImportResolved(Ok(Box::new(sample_media(true))))
        )
        .is_some());
        assert!(update(&mut state, &Message::LinkImportConfirm).is_some());
        assert!(state.link_import.is_none());
        assert!(state.transcribe_status.is_some());
    }

    #[test]
    fn parse_clip_range_needs_both_bounds() {
        assert!(parse_clip_range("", "").is_none());
        assert!(parse_clip_range("1:00", "").is_none());
        let clip = parse_clip_range("1:00", "1:30").expect("both bounds parse");
        assert_eq!(clip.start_secs, 60.0);
        assert_eq!(clip.end_secs, 90.0);
    }

    #[test]
    fn parse_clip_range_rejects_empty_or_inverted_ranges() {
        assert!(parse_clip_range("1:30", "1:00").is_none());
        assert!(parse_clip_range("1:00", "1:00").is_none());
    }

    #[test]
    fn update_ignores_unrelated_messages() {
        let mut state = open_state();
        assert!(update(&mut state, &Message::ThemeToggled).is_none());
    }
}
