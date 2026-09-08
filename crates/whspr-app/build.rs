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

use std::path::Path;
use std::sync::Arc;

/// The rasterized window icon's side length in pixels. Plenty for a
/// titlebar/taskbar icon; `crate::hub::window_icon` must use the same
/// value to interpret the raw RGBA bytes this writes.
const ICON_SIZE: u32 = 512;

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let svg_path = Path::new(&manifest_dir).join("assets/icon.svg");
    let font_path = Path::new(&manifest_dir).join("assets/fonts/Archivo-ExtraBold.ttf");
    println!("cargo:rerun-if-changed={}", svg_path.display());
    println!("cargo:rerun-if-changed={}", font_path.display());

    let svg_data = std::fs::read(&svg_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", svg_path.display()));

    // The icon's "w" is set in Archivo ExtraBold (see `assets/icon.svg`'s
    // `font-family`/`font-weight`); load the same vendored face the app
    // itself renders with (`crate::theme::fonts::ARCHIVO_EXTRABOLD`) so the
    // rasterized icon isn't at the mercy of whatever fonts happen to be
    // installed on the build machine.
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb
        .load_font_file(&font_path)
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
    let out_dir = std::env::var("OUT_DIR").expect("cargo always sets OUT_DIR");
    let out_path = Path::new(&out_dir).join("icon.rgba");
    std::fs::write(&out_path, pixmap.data())
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));
}
