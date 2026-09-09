//! The in-memory `@local/whspr:0.2.0` Typst package.
//!
//! The package is never written to disk: [`crate::world::WhsprWorld`] serves
//! these two `const` strings straight out of memory when the compiler resolves
//! `@local/whspr:...`. Keeping the package source as Rust constants (rather
//! than committed `.typ` files) is deliberate — it leaves the Nix crane source
//! filter (Rust/Cargo + `.svg` only) untouched.
//!
//! The package exports a single show-rule, [`lecture`], plus a [`point`]
//! function for timestamped note lines. The three note *templates* build on
//! top of these primitives (see [`crate::templates`]).

/// The package manifest (`typst.toml`). Only `name`, `version`, and
/// `entrypoint` are required by Typst's `PackageManifest`; the compiler
/// validates that this name/version matches the `@local/whspr:0.2.0` import.
pub(crate) const PACKAGE_MANIFEST: &str = r#"[package]
name = "whspr"
version = "0.2.0"
entrypoint = "lib.typ"
"#;

/// The package entrypoint (`lib.typ`): the `lecture` show-rule and the `point`
/// function. Both lean only on font families shipped by `typst-assets`
/// (Libertinus Serif, DejaVu Sans Mono) so notes render even before the app
/// hands the compiler its own embedded faces.
pub(crate) const PACKAGE_LIB: &str = r#"// whspr note primitives, served in-memory as `@local/whspr:0.2.0`.

#let lecture(
  title: "",
  speaker: "",
  source: "",
  date: "",
  body,
) = {
  set page(paper: "a4", margin: (x: 2cm, y: 2.2cm))
  set text(font: "Libertinus Serif", size: 11pt)
  set par(justify: true)
  show heading.where(level: 2): it => {
    set text(size: 13pt, weight: "bold")
    block(above: 1.1em, below: 0.5em, it.body)
  }

  block(width: 100%, text(size: 20pt, weight: "bold", title))
  let meta = (speaker, source, date).filter(s => s != "").join("  \u{00B7}  ")
  if meta != none {
    text(size: 9.5pt, fill: luma(110), meta)
  }
  v(0.3em)
  line(length: 100%, stroke: 0.5pt + luma(200))
  v(0.6em)

  body
}

#let point(t: "", body) = {
  grid(
    columns: (auto, 1fr),
    column-gutter: 0.8em,
    row-gutter: 0.4em,
    text(font: "DejaVu Sans Mono", size: 8.5pt, fill: luma(120), t),
    body,
  )
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_declares_name_version_entrypoint() {
        assert!(PACKAGE_MANIFEST.contains("name = \"whspr\""));
        assert!(PACKAGE_MANIFEST.contains("version = \"0.2.0\""));
        assert!(PACKAGE_MANIFEST.contains("entrypoint = \"lib.typ\""));
    }

    #[test]
    fn lib_exports_lecture_and_point() {
        assert!(PACKAGE_LIB.contains("#let lecture("));
        assert!(PACKAGE_LIB.contains("#let point("));
    }
}
