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

impl NoteDeskState {
    /// Builds a note desk from a finished link import: `title` heads the desk,
    /// `headings` are the kept chapters, and each transcript segment becomes a
    /// candidate [`TranscriptRow`] (start time as `MM:SS`, the segment's own
    /// speaker if it carries one). A transcript with no segments -- e.g. the
    /// mock ASR, which only fills `text` -- collapses to a single `00:00` row
    /// so the desk is never empty. The filter starts at its `Key` default.
    pub fn from_import(
        title: &str,
        headings: Vec<NoteHeading>,
        transcript: &whspr_core::Transcript,
    ) -> Self {
        let rows: Vec<TranscriptRow> = if transcript.segments.is_empty() {
            if transcript.text.trim().is_empty() {
                Vec::new()
            } else {
                vec![TranscriptRow {
                    time_label: "00:00".to_string(),
                    text: transcript.text.clone(),
                    speaker_id: None,
                    gutter: Gutter::Candidate,
                    keep_score: 2,
                }]
            }
        } else {
            transcript
                .segments
                .iter()
                .map(|seg| TranscriptRow {
                    time_label: secs_to_mmss(seg.start_secs),
                    text: seg.text.clone(),
                    speaker_id: seg.speaker.clone(),
                    gutter: Gutter::Candidate,
                    keep_score: 2,
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

/// Generates the source and kicks off a save-dialog + `typst compile` for a
/// PDF off the UI thread. A no-op if the desk has closed.
fn start_export_pdf(state: &mut State) -> Task<Message> {
    let Some(nd) = state.note_desk.as_mut() else {
        return Task::none();
    };
    let source = crate::note_export::document_typ(nd);
    let file_name = suggested_file_name(&nd.title, "pdf");
    nd.export_status = Some("Exporting PDF\u{2026}".to_string());
    Task::perform(save_pdf(source, file_name), Message::NoteDeskExportPdfDone)
}

/// The PDF save path: opens a native save dialog (cancel -> `Ok(None)`),
/// writes the Typst to a temp file, then shells out to `typst compile`. The
/// compile runs on a blocking thread so it never stalls the async runtime,
/// and the temp file is removed either way.
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
    let tmp = temp_typ_path();
    std::fs::write(&tmp, source).map_err(|e| e.to_string())?;
    let compile = {
        let (tmp, pdf_path) = (tmp.clone(), pdf_path.clone());
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("typst")
                .arg("compile")
                .arg(&tmp)
                .arg(&pdf_path)
                .output()
        })
        .await
    };
    let _ = std::fs::remove_file(&tmp);
    let output = compile
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("could not run typst: {e}"))?;
    if output.status.success() {
        Ok(Some(pdf_path))
    } else {
        Err(format!(
            "typst compile failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
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
        .map(|c| if matches!(c, '/' | '\\') || c.is_control() { '-' } else { c })
        .collect();
    let stem = stem.trim();
    let stem = if stem.is_empty() { "note" } else { stem };
    format!("{stem}.{ext}")
}

/// A unique scratch path for the `.typ` handed to `typst compile`.
fn temp_typ_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("whspr-note-{}-{nanos}.typ", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_config::Config;

    #[test]
    fn enter_note_desk_seeds_the_mode() {
        let mut state = State::new(Config::default());
        assert!(state.note_desk.is_none());
        assert!(update(&mut state, &Message::EnterNoteDesk).is_some());
        let nd = state.note_desk.as_ref().expect("note desk seeded");
        assert!(!nd.rows.is_empty());
    }

    #[test]
    fn back_to_dictate_clears_the_mode() {
        let mut state = State::new(Config::default());
        state.note_desk = Some(NoteDeskState::sample());
        assert!(update(&mut state, &Message::BackToDictate).is_some());
        assert!(state.note_desk.is_none());
    }

    #[test]
    fn set_transcript_filter_updates_the_active_desk() {
        let mut state = State::new(Config::default());
        state.note_desk = Some(NoteDeskState::sample());
        let msg = Message::SetTranscriptFilter(TranscriptFilter::Kept);
        assert!(update(&mut state, &msg).is_some());
        assert_eq!(
            state.note_desk.as_ref().expect("desk").filter,
            TranscriptFilter::Kept
        );
    }

    #[test]
    fn update_ignores_unrelated_messages() {
        let mut state = State::new(Config::default());
        assert!(update(&mut state, &Message::ThemeToggled).is_none());
    }

    #[test]
    fn toggle_view_code_flips_the_flag() {
        let mut state = State::new(Config::default());
        state.note_desk = Some(NoteDeskState::sample());
        assert!(!state.note_desk.as_ref().unwrap().view_code);
        assert!(update(&mut state, &Message::NoteDeskToggleViewCode).is_some());
        assert!(state.note_desk.as_ref().unwrap().view_code);
        assert!(update(&mut state, &Message::NoteDeskToggleViewCode).is_some());
        assert!(!state.note_desk.as_ref().unwrap().view_code);
    }

    #[test]
    fn export_done_ok_records_the_saved_path() {
        let mut state = State::new(Config::default());
        state.note_desk = Some(NoteDeskState::sample());
        let path = PathBuf::from("/tmp/note.typ");
        assert!(update(&mut state, &Message::NoteDeskExportTypDone(Ok(Some(path)))).is_some());
        assert_eq!(
            state.note_desk.as_ref().unwrap().export_status.as_deref(),
            Some("Saved Typst to /tmp/note.typ")
        );
    }

    #[test]
    fn export_done_cancel_clears_status() {
        let mut state = State::new(Config::default());
        let mut nd = NoteDeskState::sample();
        nd.export_status = Some("Exporting PDF\u{2026}".to_string());
        state.note_desk = Some(nd);
        assert!(update(&mut state, &Message::NoteDeskExportPdfDone(Ok(None))).is_some());
        assert!(state.note_desk.as_ref().unwrap().export_status.is_none());
    }

    #[test]
    fn export_done_err_records_the_failure() {
        let mut state = State::new(Config::default());
        state.note_desk = Some(NoteDeskState::sample());
        assert!(update(
            &mut state,
            &Message::NoteDeskExportPdfDone(Err("boom".to_string()))
        )
        .is_some());
        assert_eq!(
            state.note_desk.as_ref().unwrap().export_status.as_deref(),
            Some("PDF export failed: boom")
        );
    }

    #[test]
    fn suggested_file_name_sanitizes_and_falls_back() {
        assert_eq!(suggested_file_name("Lecture 7", "typ"), "Lecture 7.typ");
        assert_eq!(suggested_file_name("a/b\\c", "pdf"), "a-b-c.pdf");
        assert_eq!(suggested_file_name("   ", "typ"), "note.typ");
    }

    #[test]
    fn sample_keep_scores_are_in_range() {
        for row in NoteDeskState::sample().rows {
            assert!(row.keep_score <= 3);
        }
    }

    #[test]
    fn sample_has_a_title_and_rows() {
        let nd = NoteDeskState::sample();
        assert!(!nd.title.is_empty());
        assert!(!nd.rows.is_empty());
    }

    #[test]
    fn from_import_maps_segments_to_candidate_rows() {
        let transcript = whspr_core::Transcript {
            text: "one two".to_string(),
            language: Some("en".to_string()),
            segments: vec![
                whspr_core::TranscriptSegment {
                    text: "one".to_string(),
                    start_secs: 5.0,
                    end_secs: 8.0,
                    speaker: Some("Speaker A".to_string()),
                },
                whspr_core::TranscriptSegment {
                    text: "two".to_string(),
                    start_secs: 65.0,
                    end_secs: 70.0,
                    speaker: None,
                },
            ],
        };
        let headings = vec![NoteHeading {
            time_label: "00:00".to_string(),
            title: "Intro".to_string(),
        }];
        let nd = NoteDeskState::from_import("Lecture", headings, &transcript);
        assert_eq!(nd.title, "Lecture");
        assert_eq!(nd.headings.len(), 1);
        assert_eq!(nd.rows.len(), 2);
        assert_eq!(nd.rows[0].time_label, "00:05");
        assert_eq!(nd.rows[0].text, "one");
        assert_eq!(nd.rows[0].speaker_id.as_deref(), Some("Speaker A"));
        assert_eq!(nd.rows[0].gutter, Gutter::Candidate);
        assert_eq!(nd.rows[1].time_label, "01:05");
        assert!(nd.rows[1].speaker_id.is_none());
    }

    /// A `TranscriptRow` with just the fields the filter reads set.
    fn row_with(gutter: Gutter, keep_score: u8) -> TranscriptRow {
        TranscriptRow {
            time_label: "00:00".to_string(),
            text: String::new(),
            speaker_id: None,
            gutter,
            keep_score,
        }
    }

    #[test]
    fn filter_all_keeps_every_row() {
        for gutter in [Gutter::Kept, Gutter::Candidate, Gutter::Chatter] {
            for score in 0..=3 {
                assert!(TranscriptFilter::All.keeps(&row_with(gutter, score)));
            }
        }
    }

    #[test]
    fn filter_kept_keeps_only_kept_rows() {
        assert!(TranscriptFilter::Kept.keeps(&row_with(Gutter::Kept, 0)));
        assert!(!TranscriptFilter::Kept.keeps(&row_with(Gutter::Candidate, 3)));
        assert!(!TranscriptFilter::Kept.keeps(&row_with(Gutter::Chatter, 3)));
    }

    #[test]
    fn filter_key_includes_high_score_candidates() {
        assert!(TranscriptFilter::Key.keeps(&row_with(Gutter::Kept, 0)));
        assert!(TranscriptFilter::Key.keeps(&row_with(Gutter::Candidate, 2)));
        assert!(!TranscriptFilter::Key.keeps(&row_with(Gutter::Candidate, 1)));
    }

    #[test]
    fn filter_default_is_key() {
        assert_eq!(TranscriptFilter::default(), TranscriptFilter::Key);
    }

    #[test]
    fn from_import_without_segments_uses_a_single_text_row() {
        let transcript = whspr_core::Transcript {
            text: "just text".to_string(),
            ..Default::default()
        };
        let nd = NoteDeskState::from_import("Talk", Vec::new(), &transcript);
        assert_eq!(nd.rows.len(), 1);
        assert_eq!(nd.rows[0].time_label, "00:00");
        assert_eq!(nd.rows[0].text, "just text");
        assert!(nd.headings.is_empty());
    }
}
