//! whspr-typst — compile auto-generated Typst notes to a live page preview and
//! export `.typ` / PDF.
//!
//! The app hands this crate two things: a structured description of a set of
//! notes ([`NotesMeta`] + [`NotePoint`]s) and, at runtime, its own embedded
//! font faces. [`generate_notes_typ`] turns the notes into a Typst document
//! that imports the in-memory `@local/whspr:0.2.0` package
//! ([`crate::package`]); [`compile_preview_svg`] renders that document to one
//! SVG per page for the live preview, and [`export_pdf`] / [`export_typ`]
//! produce the shippable artifacts.
//!
//! Nothing here reads from disk or the network, and no Typst package is
//! committed as a `.typ` file — the package and templates live as Rust
//! constants so the Nix crane source filter stays unchanged. Every fallible
//! path maps into [`whspr_core::WhsprError`].

mod generate;
mod package;
mod templates;
mod world;

pub use generate::{export_typ, export_typ_to_path, generate_notes_typ, NotePoint, NotesMeta};
pub use templates::Template;
pub use world::{compile_preview_svg, export_pdf};
