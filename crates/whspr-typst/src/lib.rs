//! whspr-typst — compile auto-generated Typst notes to a live page preview
//! and export `.typ` / PDF.
//!
//! The crate is built up module by module: the in-memory `@local/whspr`
//! package ([`crate::package`]), the note templates, the source generator, and
//! the Typst [`World`](world) that renders them.

mod generate;
mod package;
mod templates;

pub use generate::{export_typ, export_typ_to_path, generate_notes_typ, NotePoint, NotesMeta};
pub use templates::Template;
