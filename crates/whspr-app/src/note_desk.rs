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

/// All state for the note-desk mode. `title` heads the desk, `rows` are the
/// transcript lines, and `timer_start` drives the header's elapsed timer. The
/// Typst preview is a static placeholder this phase (real rendering is a later
/// phase), so it carries no field yet.
#[derive(Debug)]
pub struct NoteDeskState {
    /// The desk title, shown in the header (e.g. "Statistical Mechanics · 7").
    pub title: String,
    /// The transcript rows shown in the left column.
    pub rows: Vec<TranscriptRow>,
    /// When the session started -- the header timer reads `elapsed()`.
    pub timer_start: std::time::Instant,
}
