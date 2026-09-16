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
    pub use_captions: bool,
    /// The caption language code the captions path would use, if any.
    pub caption_lang: Option<String>,
    /// Per-chapter "include as a note heading" flags; all-true on resolve.
    pub chapters_included: Vec<bool>,
    /// Live contents of the "clip from" `MM:SS` input.
    pub clip_start: String,
    /// Live contents of the "clip to" `MM:SS` input.
    pub clip_end: String,
    /// A status shown in the footer while an import runs (also disabling the
    /// confirm button); `None` when idle.
    pub importing: Option<String>,
    /// Whisper progress (0..=100) for the transcribe path; `None` until it reports.
    pub import_progress: Option<u8>,
    /// The decoded thumbnail handle (created once to avoid per-frame re-decode
    /// flicker); `None` shows the placeholder.
    pub thumbnail: Option<iced::widget::image::Handle>,
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
        Message::LinkImportKey(event) => {
            // Esc dismisses the modal, like Cancel -- the dialog otherwise
            // only closed via its button. Other keys are ignored here.
            if let iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            } = event
            {
                state.link_import = None;
            }
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
            Some(thumbnail_task(state))
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
        Message::LinkImportConfirm => Some(start_import(state)),
        Message::LinkImportTranscribeToDictate => Some(start_transcribe_to_dictate(state)),
        Message::LinkImportImported(result) => {
            apply_imported(state, result);
            Some(Task::none())
        }
        Message::LinkImportProgress {
            downloading,
            percent,
        } => {
            if let Some(li) = state.link_import.as_mut() {
                li.import_progress = Some(*percent);
                li.importing = Some(
                    if *downloading {
                        "Downloading audio\u{2026}"
                    } else {
                        "Transcribing\u{2026}"
                    }
                    .to_string(),
                );
            }
            Some(Task::none())
        }
        Message::LinkImportThumbnail(bytes) => {
            if let Some(li) = state.link_import.as_mut() {
                li.thumbnail = bytes.clone().map(iced::widget::image::Handle::from_bytes);
            }
            Some(Task::none())
        }
        _ => None,
    }
}

/// The cookie source for import spawns, from the persisted Privacy setting
/// (`Settings -> Privacy -> Media-import sign-in`): a chosen browser's
/// logged-in session, or anonymous.
fn cookies_from(config: &whspr_config::Config) -> whspr_import::CookiesFrom {
    match &config.privacy.cookies_browser {
        Some(browser) => whspr_import::CookiesFrom::Browser(browser.clone()),
        None => whspr_import::CookiesFrom::None,
    }
}

/// Kicks off `whspr_import::resolve` for the current URL off the UI thread,
/// marking the dialog `resolving`. A blank URL is a no-op. yt-dlp's error is
/// mapped to `String` so it lands in `LinkImportResolved(Err(..))`.
fn start_resolve(state: &mut State) -> Task<Message> {
    let cookies = cookies_from(&state.config);
    let Some(li) = state.link_import.as_mut() else {
        return Task::none();
    };
    let url = li.url.trim().to_string();
    if url.is_empty() {
        return Task::none();
    }
    li.resolving = true;
    li.error = None;
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

/// Fetches the resolved media's thumbnail off the UI thread (best-effort);
/// a no-op without a live dialog, media, or thumbnail URL.
fn thumbnail_task(state: &State) -> Task<Message> {
    let Some(li) = state.link_import.as_ref() else {
        return Task::none();
    };
    let Some(media) = li.media.as_ref() else {
        return Task::none();
    };
    let Some(thumb_url) = media.thumbnail.clone() else {
        return Task::none();
    };
    Task::perform(
        async move { whspr_import::download_thumbnail(&thumb_url).await.ok() },
        Message::LinkImportThumbnail,
    )
}

/// Folds a finished `resolve` into the dialog: stores the media, seeds every
/// chapter included, and picks the default import path; on failure records the
/// error and clears the media.
fn apply_resolved(state: &mut State, result: &Result<Box<whspr_import::MediaInfo>, String>) {
    let Some(li) = state.link_import.as_mut() else {
        return;
    };
    li.resolving = false;
    match result {
        Ok(media) => {
            li.chapters_included = vec![true; media.chapters.len()];
            // Default to the caption path whenever any track exists (published
            // first, else the source auto-caption): instant, model-free, and —
            // fetched by direct URL — importable even when the audio is gated.
            let default_track = media
                .human_captions
                .first()
                .or_else(|| media.auto_captions.first());
            li.use_captions = default_track.is_some();
            li.caption_lang = default_track.map(|l| l.code.clone());
            li.error = None;
            li.thumbnail = None;
            li.media = Some(media.as_ref().clone());
        }
        Err(error) => {
            li.error = Some(error.clone());
            li.media = None;
        }
    }
}

/// Folds a finished import: on success builds the note desk and closes the
/// dialog; on failure keeps it open with the error. Clears the status either way.
fn apply_imported(state: &mut State, result: &Result<Box<ImportedNote>, String>) {
    state.transcribe_status = None;
    match result {
        Ok(payload) => {
            let (title, headings, transcript) = payload.as_ref();
            // An import is a transcription too: record it to History (with the
            // video title as a prefix so the row is identifiable), like the
            // record/file-transcribe paths do.
            let duration = transcript.segments.last().map(|s| s.end_secs);
            let history_text = format!("{title}\n{}", transcript.text);
            crate::history::record_completed(state, history_text, duration, None);
            state.note_desk = Some(NoteDeskState::from_import(
                title,
                headings.clone(),
                transcript,
            ));
            state.link_import = None;
        }
        Err(error) => {
            if let Some(li) = state.link_import.as_mut() {
                li.error = Some(error.clone());
                li.importing = None;
                li.import_progress = None;
            }
        }
    }
}

/// Runs the chosen import off the UI thread → [`Message::LinkImportImported`].
/// Captions return a `Transcript` directly; transcribe downloads audio, runs
/// the ASR backend, then deletes the temp WAV. No-op if unresolved/blank.
fn start_import(state: &mut State) -> Task<Message> {
    let Some(li) = state.link_import.as_ref() else {
        return Task::none();
    };
    let Some(media) = li.media.as_ref() else {
        return Task::none();
    };
    let url = li.url.trim().to_string();
    if url.is_empty() {
        return Task::none();
    }
    let use_captions = li.use_captions;
    let title = media.title.clone();
    let headings: Vec<NoteHeading> = media
        .chapters
        .iter()
        .enumerate()
        .filter(|(i, _)| li.chapters_included.get(*i).copied().unwrap_or(true))
        .map(|(_, ch)| NoteHeading {
            time_label: crate::note_desk::secs_to_mmss(ch.start_secs),
            title: ch.title.clone(),
        })
        .collect();
    let caption_lang = li.caption_lang.clone();
    // The Lang the pick landed on (published first), kept for its direct URL.
    let selected_track = caption_lang.as_ref().and_then(|code| {
        media
            .human_captions
            .iter()
            .chain(media.auto_captions.iter())
            .find(|l| &l.code == code)
            .cloned()
    });
    let human_track_selected = media
        .human_captions
        .iter()
        .any(|l| Some(&l.code) == caption_lang.as_ref());
    let clip = parse_clip_range(&li.clip_start, &li.clip_end);
    let cookies = cookies_from(&state.config);
    let config = state.config.clone();

    if let Some(li) = state.link_import.as_mut() {
        li.error = None;
        li.import_progress = None;
        li.importing = Some(
            if use_captions {
                "Fetching captions\u{2026}"
            } else {
                "Downloading audio + transcribing\u{2026}"
            }
            .to_string(),
        );
    }
    state.transcribe_status = Some("Importing\u{2026}".to_string());

    // The transcribe path reports on two channels: audio download, then whisper.
    let (dl_tx, dl_rx) = tokio::sync::mpsc::unbounded_channel::<u8>();
    let (tr_tx, tr_rx) = tokio::sync::mpsc::unbounded_channel::<u8>();

    let import = Task::perform(
        async move {
            let transcript = if use_captions {
                // Prefer the track's direct URL; fall back to a yt-dlp spawn.
                match selected_track
                    .as_ref()
                    .and_then(|t| Some((t.url.as_deref()?, t.ext.as_deref().unwrap_or(""))))
                {
                    Some((track_url, ext)) => whspr_import::fetch_caption(track_url, ext)
                        .await
                        .map_err(|e| e.to_string())?,
                    None => {
                        let lang = caption_lang.unwrap_or_else(|| "en".to_string());
                        whspr_import::download_captions(&url, &lang, !human_track_selected, cookies)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                }
            } else {
                // Both phases share the bar: download fills it 0..100, then whisper.
                let (wav, audio) =
                    whspr_import::download_to_audio(&url, clip, cookies, Some(dl_tx))
                        .await
                        .map_err(|e| e.to_string())?;
                let asr = crate::worker::build_asr_backend(&config)?;
                let language =
                    whspr_config::effective_language(&config.language_settings, &config.language);
                let opts = whspr_core::AsrOptions {
                    language,
                    translate: config.capture.translate,
                };
                let mut transcript = asr
                    .transcribe_with_progress(&audio, &opts, tr_tx)
                    .await
                    .map_err(|e| e.to_string())?;
                // Best-effort speaker labels (no-op without a diarization model).
                crate::speakers::attribute_transcript(&mut transcript, &audio, &config).await;
                let _ = std::fs::remove_file(&wav);
                transcript
            };
            Ok::<_, String>(Box::new((title, headings, transcript)))
        },
        Message::LinkImportImported,
    );

    Task::batch([
        import,
        import_progress_task(dl_rx, true),
        import_progress_task(tr_rx, false),
    ])
}

/// Downloads the resolved link's audio and transcribes it straight into the
/// Dictate transcript + History, routed through [`Message::FileTranscribed`]
/// so it reuses the exact file-transcribe display, history record, and speaker
/// attribution (see `crate::transcribe_url`). Unlike [`start_import`], this
/// lands a plain transcript on the Dictate screen instead of building a note
/// desk, so it closes the dialog and switches to Dictate where that transcript
/// shows. No-op if nothing has resolved yet or the URL is blank.
fn start_transcribe_to_dictate(state: &mut State) -> Task<Message> {
    let Some(li) = state.link_import.as_ref() else {
        return Task::none();
    };
    let Some(media) = li.media.as_ref() else {
        return Task::none();
    };
    let url = li.url.trim().to_string();
    if url.is_empty() {
        return Task::none();
    }
    let title = media.title.clone();
    let cookies = cookies_from(&state.config);
    let config = state.config.clone();

    // Close the dialog and reveal the Dictate screen so the transcript lands
    // where a file transcription would; seed the same in-flight status the
    // file-picker path shows (`Message::FileTranscribed` then completes it).
    state.link_import = None;
    state.screen = crate::state::Screen::Dictate;
    state.transcribed_text = None;
    state.transcribe_status = Some(format!("Transcribing {title}\u{2026}"));

    Task::perform(
        crate::transcribe_url::run_transcribe_url(url, cookies, config),
        Message::FileTranscribed,
    )
}

/// Bridges an import progress channel (audio download when `downloading`, else
/// whisper) into iced messages; ends when the import drops its sender.
fn import_progress_task(
    rx: tokio::sync::mpsc::UnboundedReceiver<u8>,
    downloading: bool,
) -> Task<Message> {
    let updates = iced::futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|percent| (percent, rx))
    });
    Task::run(updates, move |percent| Message::LinkImportProgress {
        downloading,
        percent,
    })
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
#[path = "link_import_tests.rs"]
mod tests;
