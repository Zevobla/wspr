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
