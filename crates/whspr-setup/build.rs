//! Copies the three static Archivo faces into `OUT_DIR` so `crate::theme`'s
//! `include_bytes!` calls embed them at compile time.
//!
//! Archivo is a general-purpose external artifact, not whspr's own source,
//! so the faces aren't vendored in git: `flake.nix` fetches them
//! hermetically by content hash (`pkgs.fetchurl`) into a directory exposed
//! as `ARCHIVO_DIR`, which this build script reads. This is the exact
//! ARCHIVO_FACES copy loop from `whspr-app`'s `build.rs` (the installer
//! reuses the app's font mechanism verbatim); it does *not* need the icon
//! rasterization that build script also performs.

use std::path::Path;

/// The three static Archivo faces `crate::theme` embeds, by the filename
/// `ARCHIVO_DIR` (see `flake.nix`'s `archivoDir` derivation) holds each
/// one under.
const ARCHIVO_FACES: [&str; 3] = [
    "Archivo-Regular.ttf",
    "Archivo-SemiBold.ttf",
    "Archivo-ExtraBold.ttf",
];

fn main() {
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

    for face in ARCHIVO_FACES {
        let src = archivo_dir.join(face);
        let dst = out_dir.join(face);
        println!("cargo:rerun-if-changed={}", src.display());
        // ARCHIVO_DIR's files are read-only Nix store paths, so a stale
        // read-only dst from a previous run can't be overwritten in place --
        // drop it first (removing a file only needs write access to the
        // directory, not the file itself).
        let _ = std::fs::remove_file(&dst);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!("failed to copy {} to {}: {e}", src.display(), dst.display())
        });
    }
}
