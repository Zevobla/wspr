//! Generating note source and persisting it as `.typ`.
//!
//! [`generate_notes_typ`] turns structured note data ([`NotesMeta`] +
//! [`NotePoint`]s) into a Typst document with the locked shape:
//!
//! ```typst
//! #import "@local/whspr:0.2.0": lecture, point
//! #show: lecture.with(title: "..", speaker: "..", source: "..", date: "..")
//! == <heading>
//! #point(t: "11:52")[<verbatim sentence>]
//! ```
//!
//! The verbatim transcript text is preserved: for ordinary prose (no Typst
//! markup metacharacters) [`escape_markup`] is the identity, so the sentence
//! survives byte-for-byte into the `#point(..)[..]` body.

use std::path::Path;

use crate::templates::Template;

/// Document-level metadata for a set of notes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotesMeta {
    /// The note title, shown in the masthead.
    pub title: String,
    /// Who spoke (lecturer, host, ...). Empty to omit.
    pub speaker: String,
    /// Where the notes came from (course, meeting, file). Empty to omit.
    pub source: String,
    /// A human date string. Empty to omit.
    pub date: String,
}

/// A single timestamped note line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotePoint {
    /// The timestamp label, e.g. `"11:52"`.
    pub t: String,
    /// The verbatim sentence for this point.
    pub text: String,
    /// An optional section heading to emit (as `== heading`) before this
    /// point.
    pub heading: Option<String>,
}

/// Render structured notes into a Typst document string.
pub fn generate_notes_typ(meta: NotesMeta, points: &[NotePoint], template: Template) -> String {
    let mut out = String::new();
    out.push_str("#import \"@local/whspr:0.2.0\": lecture, point\n");
    out.push_str(template.prelude());
    out.push_str(&format!(
        "#show: lecture.with(title: {}, speaker: {}, source: {}, date: {})\n\n",
        typst_string(&meta.title),
        typst_string(&meta.speaker),
        typst_string(&meta.source),
        typst_string(&meta.date),
    ));
    for point in points {
        if let Some(heading) = &point.heading {
            out.push_str("== ");
            out.push_str(&escape_markup(heading));
            out.push('\n');
        }
        out.push_str(&format!(
            "#point(t: {})[{}]\n",
            typst_string(&point.t),
            escape_markup(&point.text),
        ));
    }
    out
}

/// Return the canonical `.typ` source for a compiled/generated document. This
/// is the exact string the app should persist next to an exported PDF.
pub fn export_typ(main_typ: &str) -> String {
    main_typ.to_string()
}

/// Write the `.typ` source to `path`, mapping IO failures into
/// [`whspr_core::WhsprError`].
pub fn export_typ_to_path(main_typ: &str, path: &Path) -> whspr_core::Result<()> {
    std::fs::write(path, main_typ)?;
    Ok(())
}

/// Escape a Rust string into a double-quoted Typst string literal.
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

/// Backslash-escape the Typst markup metacharacters so arbitrary transcript
/// text is safe inside a `[..]` content body. Ordinary prose contains none of
/// these, so this is the identity for the common case and the sentence is
/// preserved verbatim.
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
    use tempfile::tempdir;

    fn sample_points() -> Vec<NotePoint> {
        vec![
            NotePoint {
                t: "11:52".to_string(),
                text: "The mitochondria is the powerhouse of the cell.".to_string(),
                heading: Some("Cell biology".to_string()),
            },
            NotePoint {
                t: "11:58".to_string(),
                text: "ATP is produced through oxidative phosphorylation.".to_string(),
                heading: None,
            },
        ]
    }

    #[test]
    fn output_imports_the_local_package() {
        let out = generate_notes_typ(NotesMeta::default(), &sample_points(), Template::Lecture);
        assert!(out.contains("#import \"@local/whspr:0.2"));
    }

    #[test]
    fn output_has_a_point_per_note_with_verbatim_text() {
        let points = sample_points();
        let out = generate_notes_typ(NotesMeta::default(), &points, Template::Lecture);
        for point in &points {
            assert!(out.contains(&format!("#point(t: \"{}\")", point.t)));
            assert!(out.contains(&point.text));
        }
    }

    #[test]
    fn headings_are_emitted_before_their_point() {
        let out = generate_notes_typ(NotesMeta::default(), &sample_points(), Template::Lecture);
        assert!(out.contains("== Cell biology"));
    }

    #[test]
    fn show_rule_carries_metadata() {
        let meta = NotesMeta {
            title: "Bio 101".to_string(),
            speaker: "Dr. Ada".to_string(),
            source: "Lecture 4".to_string(),
            date: "2026-09-09".to_string(),
        };
        let out = generate_notes_typ(meta, &sample_points(), Template::Minutes);
        assert!(out.contains("#show: lecture.with(title: \"Bio 101\""));
        assert!(out.contains("speaker: \"Dr. Ada\""));
    }

    #[test]
    fn export_typ_round_trips_the_source() {
        let src = "#show: lecture\n";
        assert_eq!(export_typ(src), src);
    }

    #[test]
    fn export_typ_to_path_writes_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.typ");
        export_typ_to_path("#set page()\n", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "#set page()\n");
    }

    #[test]
    fn markup_metacharacters_are_escaped() {
        assert_eq!(escape_markup("a #b [c]"), "a \\#b \\[c\\]");
        assert_eq!(escape_markup("plain prose"), "plain prose");
    }
}
