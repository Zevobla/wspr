//! The longform "note desk" mode: its domain types, sample data, and the
//! `update` handler that toggles the mode. The full-screen view lives in
//! `crate::hub::note_desk`; this module owns the types that view renders and
//! the two messages that enter/leave the desk.
//!
//! Entered and left manually for now -- auto-morph, live transcription and
//! real Typst rendering are later phases. The handler is chained through
//! `crate::app::update`'s catch-all (the same way `crate::hf::update` and
//! `crate::hub::settings::update` are) so app.rs stays under its line cap and
//! carries no note-desk arms inline.

use std::path::PathBuf;

use iced::Task;

use crate::state::{Message, State};

/// A transcript row's keep-gutter state -- the left-edge mark showing whether
/// a line was kept into the notes, is a candidate awaiting a keep/dismiss
/// decision, or is chatter left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gutter {
    /// Kept into the notes: a filled accent square plus an inset accent rule
    /// down the row's left edge.
    Kept,
    /// A candidate awaiting a keep/dismiss decision: a 2px accent-outline
    /// square.
    Candidate,
    /// Chatter left out of the notes: no gutter mark.
    Chatter,
}

/// One line of the live transcript column.
#[derive(Debug, Clone)]
pub struct TranscriptRow {
    /// The line's timestamp, formatted `MM:SS`.
    pub time_label: String,
    /// The recognized text of the line.
    pub text: String,
    /// The speaker this line is attributed to, if any. A run of rows that
    /// share a speaker is topped by one uppercase speaker label.
    pub speaker_id: Option<String>,
    /// The keep-gutter state (see [`Gutter`]).
    pub gutter: Gutter,
    /// How strongly the line scored for keeping, `0..=3` -- rendered as up to
    /// three accent dots (the remainder dim).
    pub keep_score: u8,
}

/// A kept chapter marker, shown as a note heading in the desk's notes column.
/// Built from the chapters the user chose to keep in the "Add from a link"
/// dialog (see `crate::link_import`); the manual-entry `sample()` desk has none.
#[derive(Debug, Clone)]
pub struct NoteHeading {
    /// The chapter's start time, formatted `MM:SS`.
    pub time_label: String,
    /// The chapter title, rendered as a note heading.
    pub title: String,
}

/// Which transcript rows the note desk's `Key / All / Kept` segmented filter
/// shows. `Key` (the default, matching the comp) is the signal-first view:
/// kept lines plus high-signal candidates; `All` shows every line; `Kept`
/// narrows to only the lines kept into the notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranscriptFilter {
    /// Kept lines plus any high-signal line (`keep_score >= 2`) -- the
    /// default, signal-first view.
    #[default]
    Key,
    /// Every transcript line, chatter included.
    All,
    /// Only the lines kept into the notes (`Gutter::Kept`).
    Kept,
}

impl TranscriptFilter {
    /// Whether `row` is shown under this filter: `All` keeps everything,
    /// `Kept` only kept rows, and `Key` kept rows plus any high-signal line
    /// (`keep_score >= 2`).
    pub fn keeps(self, row: &TranscriptRow) -> bool {
        match self {
            TranscriptFilter::All => true,
            TranscriptFilter::Kept => row.gutter == Gutter::Kept,
            TranscriptFilter::Key => row.gutter == Gutter::Kept || row.keep_score >= 2,
        }
    }
}

/// All state for the note-desk mode: `title` heads the desk, `rows` are the
/// transcript lines, `headings` the kept chapters, and `filter` which rows the
/// transcript column shows.
#[derive(Debug)]
pub struct NoteDeskState {
    /// The desk title, shown in the header (e.g. "Statistical Mechanics · 7").
    pub title: String,
    /// The transcript rows shown in the left column.
    pub rows: Vec<TranscriptRow>,
    /// Kept chapter headings (from a link import), shown in the notes column;
    /// empty for the manual-entry `sample()` desk.
    pub headings: Vec<NoteHeading>,
    /// Which rows the `Key / All / Kept` segmented filter is showing (see
    /// [`TranscriptFilter`]); `Key` by default, matching the comp.
    pub filter: TranscriptFilter,
    /// Whether the Typst column shows the raw `.typ` source (monospace) rather
    /// than the rendered note. Toggled by the "View code"/"View note" button
    /// (see `crate::hub::note_desk::notes_pane`).
    pub view_code: bool,
    /// The most recent `.typ`/PDF export outcome, shown in the Typst column's
    /// header. `None` until an export runs (or is cancelled).
    pub export_status: Option<String>,
}

/// Formats a timestamp in seconds as `MM:SS` (minutes uncapped, e.g. `73:04`).
pub(crate) fn secs_to_mmss(secs: f32) -> String {
    let total = secs.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// A lightweight "how key is this line" heuristic (until an LLM ranking pass):
/// filler → chatter, long / figure-bearing lines → kept, the rest → candidates.
/// Drives the gutter, score dots, and `Key`/`Kept` filters (ranked, not flat).
fn rank_line(text: &str) -> (Gutter, u8) {
    let words = text.split_whitespace().count();
    if words <= 3 {
        return (Gutter::Chatter, 0);
    }
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let score = if words >= 14 || (words >= 9 && has_digit) {
        3
    } else if words >= 8 {
        2
    } else {
        1
    };
    let gutter = if score >= 3 {
        Gutter::Kept
    } else {
        Gutter::Candidate
    };
    (gutter, score)
}

impl NoteDeskState {
    /// Builds a note desk from a finished import: `title` heads the desk,
    /// `headings` are the kept chapters, and each transcript segment becomes a
    /// ranked [`TranscriptRow`] (start time, its own speaker if any). A
    /// transcript with no segments collapses to a single `00:00` row.
    pub fn from_import(
        title: &str,
        headings: Vec<NoteHeading>,
        transcript: &whspr_core::Transcript,
    ) -> Self {
        let rows: Vec<TranscriptRow> = if transcript.segments.is_empty() {
            if transcript.text.trim().is_empty() {
                Vec::new()
            } else {
                let (gutter, keep_score) = rank_line(&transcript.text);
                vec![TranscriptRow {
                    time_label: "00:00".to_string(),
                    text: transcript.text.clone(),
                    speaker_id: None,
                    gutter,
                    keep_score,
                }]
            }
        } else {
            transcript
                .segments
                .iter()
                .map(|seg| {
                    let (gutter, keep_score) = rank_line(&seg.text);
                    TranscriptRow {
                        time_label: secs_to_mmss(seg.start_secs),
                        text: seg.text.clone(),
                        speaker_id: seg.speaker.clone(),
                        gutter,
                        keep_score,
                    }
                })
                .collect()
        };
        Self {
            title: title.to_string(),
            headings,
            rows,
            filter: TranscriptFilter::default(),
            view_code: false,
            export_status: None,
        }
    }

    /// A seeded note desk with sample rows so the layout renders before live
    /// transcription exists (that arrives in a later phase). Mirrors the
    /// design comp's "Statistical Mechanics · 7 / Microstates" excerpt.
    pub fn sample() -> Self {
        let halden = Some("Dr. E. Halden".to_string());
        let student = Some("Student".to_string());
        Self {
            title: "Statistical Mechanics · 7".to_string(),
            filter: TranscriptFilter::default(),
            headings: vec![],
            view_code: false,
            export_status: None,
            rows: vec![
                TranscriptRow {
                    time_label: "11:52".to_string(),
                    text: "Entropy is not a measure of disorder — that word does \
                           more harm than good here."
                        .to_string(),
                    speaker_id: halden.clone(),
                    gutter: Gutter::Kept,
                    keep_score: 3,
                },
                TranscriptRow {
                    time_label: "12:04".to_string(),
                    text: "It counts the number of microstates consistent with \
                           what you can actually measure."
                        .to_string(),
                    speaker_id: halden.clone(),
                    gutter: Gutter::Kept,
                    keep_score: 3,
                },
                TranscriptRow {
                    time_label: "12:19".to_string(),
                    text: "So the same glass of water has a different entropy \
                           depending on what you claim to know about it."
                        .to_string(),
                    speaker_id: halden.clone(),
                    gutter: Gutter::Candidate,
                    keep_score: 2,
                },
                TranscriptRow {
                    time_label: "12:31".to_string(),
                    text: "Write that down, because that sentence is the whole \
                           of the second law."
                        .to_string(),
                    speaker_id: halden.clone(),
                    gutter: Gutter::Candidate,
                    keep_score: 3,
                },
                TranscriptRow {
                    time_label: "12:44".to_string(),
                    text: "Does that mean entropy is subjective?".to_string(),
                    speaker_id: student,
                    gutter: Gutter::Chatter,
                    keep_score: 1,
                },
                TranscriptRow {
                    time_label: "12:58".to_string(),
                    text: "It means it is a statement about what you know, and \
                           the second law is a statement about probability"
                        .to_string(),
                    speaker_id: halden,
                    gutter: Gutter::Chatter,
                    keep_score: 0,
                },
            ],
        }
    }
}

/// Handles the note-desk messages, returning `Some(task)` when it owns the
/// message and `None` otherwise so `crate::app::update`'s catch-all keeps
/// forwarding to the other handlers. Entering seeds sample rows (real
/// transcript is a later phase); leaving drops the mode entirely, restoring
/// the normal Hub shell instantly (no animation this phase).
pub fn update(state: &mut State, message: &Message) -> Option<Task<Message>> {
    match message {
        Message::EnterNoteDesk => {
            state.note_desk = Some(NoteDeskState::sample());
            Some(Task::none())
        }
        Message::BackToDictate => {
            state.note_desk = None;
            Some(Task::none())
        }
        Message::SetTranscriptFilter(filter) => {
            if let Some(nd) = state.note_desk.as_mut() {
                nd.filter = *filter;
            }
            Some(Task::none())
        }
        Message::NoteDeskToggleViewCode => {
            if let Some(nd) = state.note_desk.as_mut() {
                nd.view_code = !nd.view_code;
            }
            Some(Task::none())
        }
        Message::NoteDeskExportTyp => Some(start_export_typ(state)),
        Message::NoteDeskExportTypDone(result) => {
            apply_export_status(state, result, "Typst");
            Some(Task::none())
        }
        Message::NoteDeskExportPdf => Some(start_export_pdf(state)),
        Message::NoteDeskExportPdfDone(result) => {
            apply_export_status(state, result, "PDF");
            Some(Task::none())
        }
        _ => None,
    }
}

/// Generates the `.typ` source now (while the desk is borrowed) and kicks off
/// a save-dialog + write off the UI thread. A no-op if the desk has closed.
fn start_export_typ(state: &mut State) -> Task<Message> {
    let Some(nd) = state.note_desk.as_mut() else {
        return Task::none();
    };
    let source = crate::note_export::document_typ(nd);
    let file_name = suggested_file_name(&nd.title, "typ");
    nd.export_status = Some("Exporting .typ\u{2026}".to_string());
    Task::perform(save_typ(source, file_name), Message::NoteDeskExportTypDone)
}

/// The `.typ` save path: opens a native save dialog (cancel -> `Ok(None)`),
/// then writes the source to the chosen path.
async fn save_typ(source: String, file_name: String) -> Result<Option<PathBuf>, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("Typst source", &["typ"])
        .set_file_name(file_name)
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    std::fs::write(&path, source).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

/// Generates the source and kicks off a save-dialog + in-process PDF compile
/// off the UI thread. A no-op if the desk has closed.
fn start_export_pdf(state: &mut State) -> Task<Message> {
    let Some(nd) = state.note_desk.as_mut() else {
        return Task::none();
    };
    let source = crate::note_export::document_typ(nd);
    let file_name = suggested_file_name(&nd.title, "pdf");
    nd.export_status = Some("Exporting PDF\u{2026}".to_string());
    Task::perform(save_pdf(source, file_name), Message::NoteDeskExportPdfDone)
}

/// The PDF save path: opens a native save dialog (cancel -> `Ok(None)`), then
/// compiles the Typst source to PDF bytes in-process
/// (`crate::note_export::export_pdf`, backed by `whspr_typst::export_pdf` --
/// no system `typst` binary involved) and writes them straight to the chosen
/// path. The compile runs on a blocking thread so it never stalls the async
/// runtime.
async fn save_pdf(source: String, file_name: String) -> Result<Option<PathBuf>, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("PDF document", &["pdf"])
        .set_file_name(file_name)
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let pdf_path = handle.path().to_path_buf();
    let pdf = tokio::task::spawn_blocking(move || crate::note_export::export_pdf(&source))
        .await
        .map_err(|e| e.to_string())??;
    std::fs::write(&pdf_path, pdf).map_err(|e| e.to_string())?;
    Ok(Some(pdf_path))
}

/// Folds an export result into the desk's status line. Cancellation
/// (`Ok(None)`) just clears any "Exporting…" note without a success line.
fn apply_export_status(state: &mut State, result: &Result<Option<PathBuf>, String>, kind: &str) {
    let Some(nd) = state.note_desk.as_mut() else {
        return;
    };
    nd.export_status = match result {
        Ok(Some(path)) => Some(format!("Saved {kind} to {}", path.display())),
        Ok(None) => None,
        Err(error) => Some(format!("{kind} export failed: {error}")),
    };
}

/// A default file name from the note title, sanitized for the save dialog.
fn suggested_file_name(title: &str, ext: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim();
    let stem = if stem.is_empty() { "note" } else { stem };
    format!("{stem}.{ext}")
}

#[cfg(test)]
#[path = "note_desk_tests.rs"]
mod tests;
