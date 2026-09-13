//! Turning a note desk into a self-contained Typst document.
//!
//! [`document_typ`] renders a [`NoteDeskState`] into standalone `.typ` source
//! the system `typst` binary can compile directly -- a title heading, the kept
//! chapters as a bullet list, and every transcript row as a timestamped
//! paragraph (kept rows carry an accent timestamp, mirroring the desk gutter).
//! Unlike `whspr-typst`'s in-process generator, this imports no `@local`
//! package, so `typst compile <in.typ> <out.pdf>` works without whspr's own
//! `World`. Arbitrary title/heading/transcript text is escaped so it can never
//! break the surrounding markup.

use crate::note_desk::{Gutter, NoteDeskState};

/// The Modernist accent hue (`#ec3013`), used to color kept rows' timestamps.
const ACCENT: &str = "#ec3013";

/// Render `nd` into a valid, self-contained Typst document.
pub fn document_typ(nd: &NoteDeskState) -> String {
    let mut out = String::new();
    // Metadata + a plain page/paragraph setup. No custom font is set: the
    // system `typst` may not have Archivo installed, and the default face
    // always compiles.
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

/// Escape a Rust string into a double-quoted Typst string literal (used for
/// `#set document(title: ..)`).
fn typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Backslash-escape Typst markup metacharacters so arbitrary transcript text
/// is safe in markup position. Ordinary prose contains none of these, so this
/// is the identity for the common case and the sentence survives verbatim.
fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '[' | ']' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_desk::{NoteDeskState, NoteHeading};

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
            assert!(doc.contains(&row.text), "missing transcript line: {}", row.text);
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

    /// End-to-end: the generated source compiles cleanly with the system
    /// `typst` binary. Skipped when `typst` isn't on PATH (e.g. a CI sandbox)
    /// so it never fails the gate where the tool is absent.
    #[test]
    fn generated_document_compiles_with_typst() {
        if std::process::Command::new("typst")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: `typst` not on PATH");
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("note.typ");
        let pdf = dir.path().join("note.pdf");
        std::fs::write(&src, document_typ(&NoteDeskState::sample())).expect("write .typ");
        let output = std::process::Command::new("typst")
            .arg("compile")
            .arg(&src)
            .arg(&pdf)
            .output()
            .expect("run typst");
        assert!(
            output.status.success(),
            "typst compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(pdf.exists(), "no PDF produced");
    }
}
