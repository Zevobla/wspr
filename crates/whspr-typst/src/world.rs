//! The Typst [`World`] whspr compiles against, and the SVG/PDF entry points.
//!
//! [`WhsprWorld`] seeds its font book from **caller-provided** font bytes
//! first, then the `typst-assets` default faces — so the app can pass its own
//! embedded Archivo faces at runtime with no build-time font coupling. It also
//! serves the in-memory `@local/whspr:0.2.0` package (see [`crate::package`])
//! by matching the file ids the compiler asks for; every other path resolves
//! to [`FileError::NotFound`].
//!
//! All failure paths map into [`whspr_core::WhsprError`]; nothing here panics
//! on caller input.

use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::ecow::EcoVec;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use typst_svg::SvgOptions;

use whspr_core::{Result, WhsprError};

use crate::package::{PACKAGE_LIB, PACKAGE_MANIFEST};

/// The namespace of the in-memory package we serve (`@local/...`).
const PKG_NAMESPACE: &str = "local";
/// The name of the in-memory package we serve (`.../whspr:...`).
const PKG_NAME: &str = "whspr";

/// Compile `main_typ` all the way to a laid-out [`PagedDocument`], loading
/// `fonts` ahead of the `typst-assets` base faces. One SVG string is produced
/// per page.
pub fn compile_preview_svg(main_typ: &str, fonts: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    let document = compile_document(main_typ, fonts)?;
    let options = SvgOptions::default();
    let pages = document
        .pages()
        .iter()
        .map(|page| typst_svg::svg(page, &options).into_bytes())
        .collect();
    Ok(pages)
}

/// Compile `main_typ` and export it as a single PDF byte stream.
pub fn export_pdf(main_typ: &str, fonts: &[Vec<u8>]) -> Result<Vec<u8>> {
    let document = compile_document(main_typ, fonts)?;
    let options = PdfOptions::default();
    typst_pdf::pdf(&document, &options)
        .map_err(|diags| WhsprError::Other(format!("typst pdf export: {}", join_diags(&diags))))
}

/// Compile `main_typ` into a paged document, mapping any fatal diagnostics
/// into a [`WhsprError`]. Warnings are ignored.
fn compile_document(main_typ: &str, fonts: &[Vec<u8>]) -> Result<PagedDocument> {
    let world = WhsprWorld::new(main_typ, fonts);
    typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|diags| WhsprError::Other(format!("typst compile: {}", join_diags(&diags))))
}

/// Flatten Typst diagnostics into a single `; `-separated message.
fn join_diags(diags: &EcoVec<SourceDiagnostic>) -> String {
    diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

/// A minimal, in-memory [`World`]: one main source, the standard library, a
/// font book seeded from caller + asset fonts, and the `@local/whspr` package.
struct WhsprWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    main_source: Source,
    main_text: String,
}

impl WhsprWorld {
    /// Build a world for `main_typ`. `fonts` are decoded first (so they win
    /// family lookups), then the `typst-assets` default faces are appended.
    fn new(main_typ: &str, fonts: &[Vec<u8>]) -> Self {
        let mut loaded: Vec<Font> = Vec::new();
        for data in fonts {
            loaded.extend(Font::iter(Bytes::new(data.clone())));
        }
        for data in typst_assets::fonts() {
            loaded.extend(Font::iter(Bytes::new(data)));
        }

        let book = FontBook::from_fonts(&loaded);
        let main_id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("`main.typ` is a valid virtual path"),
        )
        .intern();
        let main_text = main_typ.to_string();
        let main_source = Source::new(main_id, main_text.clone());

        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts: loaded,
            main_id,
            main_source,
            main_text,
        }
    }
}

/// Does `id` point at the file named `name` inside our `@local/whspr` package?
fn is_pkg_file(id: FileId, name: &str) -> bool {
    match id.package() {
        Some(spec) => {
            spec.namespace.as_str() == PKG_NAMESPACE
                && spec.name.as_str() == PKG_NAME
                && id.vpath().file_name() == Some(name)
        }
        None => false,
    }
}

/// The path a `NotFound` error reports for an unresolvable `id`.
fn not_found(id: FileId) -> FileError {
    FileError::NotFound(id.vpath().as_rootless_path().to_path_buf())
}

impl World for WhsprWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            return Ok(self.main_source.clone());
        }
        if is_pkg_file(id, "lib.typ") {
            return Ok(Source::new(id, PACKAGE_LIB.to_string()));
        }
        Err(not_found(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main_id {
            return Ok(Bytes::from_string(self.main_text.clone()));
        }
        if is_pkg_file(id, "typst.toml") {
            return Ok(Bytes::new(PACKAGE_MANIFEST.as_bytes()));
        }
        if is_pkg_file(id, "lib.typ") {
            return Ok(Bytes::new(PACKAGE_LIB.as_bytes()));
        }
        Err(not_found(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_seeds_fonts_from_assets_alone() {
        let world = WhsprWorld::new("Hello", &[]);
        assert!(!world.fonts.is_empty());
        assert!(world.font(0).is_some());
    }

    #[test]
    fn compile_trivial_document_yields_one_page() {
        let pages = compile_preview_svg("Hello, whspr.", &[]).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(!pages[0].is_empty());
    }

    #[test]
    fn compile_error_maps_to_whspr_error() {
        // `#let` with no body is a parse/eval error -> fatal diagnostic.
        let err = compile_preview_svg("#let x =", &[]).unwrap_err();
        assert!(matches!(err, WhsprError::Other(_)));
    }
}
