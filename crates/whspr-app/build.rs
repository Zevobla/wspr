//! Rasterizes `assets/icon.svg` -- the single vector source for the app/
//! window icon (see that file and `assets/app-icon/README.md` for the
//! design directions it was picked from) -- into flat RGBA8 pixels at build
//! time, embedded straight into the binary. Rasters are a build artifact,
//! never checked into git (see the repo `.gitignore`): the icon's *shape*
//! lives in source control as a vector, and only becomes pixels here.
//!
//! `crate::hub::window_icon` picks the result up via
//! `include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba"))` and hands it to
//! `iced::window::icon::from_rgba`. `resvg` re-exports the exact `usvg` +
//! `tiny-skia` versions it was built against, so this only needs the one
//! build-dependency.
//!
//! This also copies the three static Archivo faces (see
//! `crate::theme::fonts`) into `OUT_DIR` for `include_bytes!` to embed.
//! Archivo is a general-purpose external artifact, not whspr's own source,
//! so it isn't vendored in git -- `flake.nix` fetches it hermetically by
//! content hash (`pkgs.fetchurl`) and points `ARCHIVO_DIR` at the assembled
//! directory (same mechanism the model-path env vars use), which this
//! build script reads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The rasterized window icon's side length in pixels. Plenty for a
/// titlebar/taskbar icon; `crate::hub::window_icon` must use the same
/// value to interpret the raw RGBA bytes this writes.
const ICON_SIZE: u32 = 512;

/// The three static Archivo faces `crate::theme::fonts` embeds, by the
/// filename `ARCHIVO_DIR` (see `flake.nix`'s `archivoDir` derivation) holds
/// each one under.
const ARCHIVO_FACES: [&str; 3] = [
    "Archivo-Regular.ttf",
    "Archivo-SemiBold.ttf",
    "Archivo-ExtraBold.ttf",
];

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let svg_path = Path::new(&manifest_dir).join("assets/icon.svg");
    println!("cargo:rerun-if-changed={}", svg_path.display());

    let archivo_dir = std::env::var("ARCHIVO_DIR").unwrap_or_else(|_| {
        panic!(
            "ARCHIVO_DIR unset -- build inside `nix develop` (or otherwise \
             point ARCHIVO_DIR at a directory containing {})",
            ARCHIVO_FACES.join(", ")
        )
    });
    println!("cargo:rerun-if-env-changed=ARCHIVO_DIR");
    let archivo_dir = Path::new(&archivo_dir);

    let out_dir = std::env::var("OUT_DIR").expect("cargo always sets OUT_DIR");
    let out_dir = Path::new(&out_dir);

    let mut face_paths: Vec<PathBuf> = Vec::with_capacity(ARCHIVO_FACES.len());
    for face in ARCHIVO_FACES {
        let src = archivo_dir.join(face);
        let dst = out_dir.join(face);
        println!("cargo:rerun-if-changed={}", src.display());
        // `fs::copy` propagates Unix permission bits from the source, and
        // ARCHIVO_DIR's files are read-only Nix store paths (fetchurl
        // output). A stale dst from a previous build-script run would
        // therefore also be read-only, and copying over it in place fails
        // with "Permission denied" -- so drop any existing copy first
        // (removing a file only needs write access to the directory, not
        // the file itself).
        let _ = std::fs::remove_file(&dst);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!("failed to copy {} to {}: {e}", src.display(), dst.display())
        });
        face_paths.push(src);
    }
    let font_path = &face_paths[2]; // Archivo-ExtraBold.ttf

    let svg_data = std::fs::read(&svg_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", svg_path.display()));

    // The icon's "w" is set in Archivo ExtraBold (see `assets/icon.svg`'s
    // `font-family`/`font-weight`); load the same face the app itself
    // renders with (`crate::theme::fonts::ARCHIVO_EXTRABOLD`) so the
    // rasterized icon isn't at the mercy of whatever fonts happen to be
    // installed on the build machine.
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb
        .load_font_file(font_path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", font_path.display()));

    let options = resvg::usvg::Options {
        fontdb: Arc::new(fontdb),
        ..Default::default()
    };

    let tree = resvg::usvg::Tree::from_data(&svg_data, &options)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", svg_path.display()));

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).expect("ICON_SIZE is non-zero");
    let source_size = tree.size().to_int_size();
    let scale = ICON_SIZE as f32 / source_size.width().max(1) as f32;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // `assets/icon.svg` is opaque edge-to-edge (a full-bleed ground rect
    // behind everything else), so tiny-skia's premultiplied-alpha pixels
    // are already identical to straight alpha here -- safe to hand
    // `pixmap.data()` to `iced::window::icon::from_rgba` as-is.
    let out_path = out_dir.join("icon.rgba");
    std::fs::write(&out_path, pixmap.data())
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));
}
