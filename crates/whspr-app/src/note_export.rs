//! Turning a note desk into a self-contained Typst document, and that
//! document into a PDF.
//!
//! [`document_typ`] renders a [`NoteDeskState`] into standalone `.typ`
//! source -- a title heading, the kept chapters as a bullet list, and every
//! transcript row as a timestamped paragraph (kept rows carry an accent
//! timestamp, mirroring the desk gutter). It imports no `@local` package, so
//! it needs nothing beyond `whspr_typst::export_pdf`'s own `World` and
//! `typst-assets` base faces to compile. [`export_pdf`] does exactly that,
//! **in-process** -- no system `typst` binary is spawned anywhere in this
//! crate.
//!
//! This document is a distinct, bespoke layout from `whspr-typst`'s own
//! generic `lecture`/`point` template ([`whspr_typst::generate_notes_typ`]):
//! the gutter-accented timestamps, kept-chapters list, and per-row speaker
//! attribution here have no equivalent in that template's metadata/point-
//! stream model, so the two aren't the same document reimplemented twice.
//! What *is* shared is lifted into `whspr-typst` proper: [`escape_markup`]/
//! [`typst_string`] (re-exported below) and, now, PDF compilation itself.
//! Arbitrary title/heading/transcript text is escaped so it can never break
//! the surrounding markup.

use whspr_typst::{escape_markup, typst_string};

use crate::note_desk::{Gutter, NoteDeskState};

/// The Modernist accent hue (`#ec3013`), used to color kept rows' timestamps.
const ACCENT: &str = "#ec3013";

/// Render `nd` into a valid, self-contained Typst document.
pub fn document_typ(nd: &NoteDeskState) -> String {
    let mut out = String::new();
    // Metadata + a plain page/paragraph setup. No custom font is set: the
    // compiled-in `typst-assets` base faces (see `export_pdf`'s doc) always
    // cover Typst's default family, so this compiles offline with no font
    // the app has to supply.
    out.push_str(&format!(
        "#set document(title: {})\n",
        typst_string(&nd.title)
    ));
    out.push_str("#set page(margin: 2cm)\n");
    out.push_str("#set par(justify: true)\n");
    out.push_str("#set text(size: 11pt)\n\n");

    // Title.
    out.push_str("= ");
    out.push_str(&escape_markup(&nd.title));
    out.push_str("\n\n");

    // Kept chapters as a bullet list, when there are any.
    if !nd.headings.is_empty() {
        out.push_str("== Chapters\n\n");
        for h in &nd.headings {
            out.push_str("- *");
            out.push_str(&escape_markup(&h.time_label));
            out.push_str("* ");
            out.push_str(&escape_markup(&h.title));
            out.push('\n');
        }
        out.push('\n');
    }

    // Transcript: one timestamped paragraph per row. Kept rows get an
    // accent-colored timestamp, the rest a dim one, so the exported note
    // echoes the desk's keep gutter.
    out.push_str("== Transcript\n\n");
    for r in &nd.rows {
        let fill = if matches!(r.gutter, Gutter::Kept) {
            format!("rgb(\"{ACCENT}\")")
        } else {
            "luma(120)".to_string()
        };
        out.push_str(&format!("#text(fill: {fill})[*"));
        out.push_str(&escape_markup(&r.time_label));
        out.push_str("*]  ");
        if let Some(speaker) = &r.speaker_id {
            out.push('_');
            out.push_str(&escape_markup(speaker));
            out.push_str(":_ ");
        }
        out.push_str(&escape_markup(&r.text));
        out.push_str("\n\n");
    }

    out
}

/// Compile `main_typ` (as produced by [`document_typ`]) straight to PDF
/// bytes, **in-process** via `whspr_typst::export_pdf` -- no system `typst`
/// binary is spawned. No caller fonts are supplied: `document_typ` never
/// sets a custom font (see its doc comment above), so Typst's default family
/// resolves against the `typst-assets` base faces `whspr-typst`'s `World`
/// always loads, and compilation succeeds fully offline with nothing the app
/// has to embed or ship itself. Failures collapse to a plain message so the
/// caller can fold them into the desk's export-status line the same way a
/// save-dialog or IO failure is.
pub fn export_pdf(main_typ: &str) -> Result<Vec<u8>, String> {
    whspr_typst::export_pdf(main_typ, &[]).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_desk::{NoteDeskState, NoteHeading, TranscriptFilter};

    #[test]
    fn document_carries_the_title() {
        let nd = NoteDeskState::sample();
        let doc = document_typ(&nd);
        assert!(doc.contains(&format!("= {}", nd.title)));
        assert!(doc.contains(&format!("#set document(title: \"{}\"", nd.title)));
    }

    #[test]
    fn document_lists_kept_chapters() {
        let mut nd = NoteDeskState::sample();
        nd.headings = vec![NoteHeading {
            time_label: "00:00".to_string(),
            title: "Intro".to_string(),
        }];
        let doc = document_typ(&nd);
        assert!(doc.contains("== Chapters"));
        assert!(doc.contains("- *00:00* Intro"));
    }

    #[test]
    fn document_has_a_transcript_paragraph_per_row() {
        let nd = NoteDeskState::sample();
        let doc = document_typ(&nd);
        assert!(doc.contains("== Transcript"));
        for row in &nd.rows {
            assert!(
                doc.contains(&row.text),
                "missing transcript line: {}",
                row.text
            );
        }
    }

    #[test]
    fn kept_rows_get_an_accent_timestamp() {
        let nd = NoteDeskState::sample();
        let doc = document_typ(&nd);
        // The sample's first row is Kept at 11:52.
        assert!(doc.contains(&format!("#text(fill: rgb(\"{ACCENT}\"))[*11:52*]")));
    }

    #[test]
    fn markup_metacharacters_in_text_are_escaped() {
        let mut nd = NoteDeskState::sample();
        nd.title = "Budget #1 [draft]".to_string();
        let doc = document_typ(&nd);
        assert!(doc.contains("= Budget \\#1 \\[draft\\]"));
    }

    #[test]
    fn typst_string_escapes_quotes_and_backslashes() {
        assert_eq!(typst_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn empty_headings_omit_the_chapters_section() {
        let mut nd = NoteDeskState::sample();
        nd.headings.clear();
        assert!(!document_typ(&nd).contains("== Chapters"));
    }

    /// End-to-end: the generated source compiles in-process to real PDF
    /// bytes, with no system `typst` binary involved (nothing here shells
    /// out at all -- see the module doc).
    #[test]
    fn sample_note_desk_compiles_to_a_pdf() {
        let pdf = export_pdf(&document_typ(&NoteDeskState::sample())).expect("pdf export");
        assert!(pdf.starts_with(b"%PDF-"), "exported bytes are not a PDF");
    }

    /// A note desk with no rows, no headings, and no title is still a
    /// syntactically valid document (`document_typ` never omits its
    /// `#set`/heading scaffolding), so it compiles to a PDF rather than
    /// erroring -- that's the honest current behaviour, not a special case.
    #[test]
    fn empty_note_desk_still_compiles_to_a_pdf() {
        let nd = NoteDeskState {
            title: String::new(),
            rows: Vec::new(),
            headings: Vec::new(),
            filter: TranscriptFilter::default(),
            view_code: false,
            export_status: None,
        };
        let pdf = export_pdf(&document_typ(&nd)).expect("empty notes still compile");
        assert!(pdf.starts_with(b"%PDF-"), "exported bytes are not a PDF");
    }
}
