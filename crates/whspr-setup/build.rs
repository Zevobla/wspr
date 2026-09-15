//! Build script for the installer. Two jobs:
//!
//! 1. **Archivo faces.** Copies the three static Archivo faces into `OUT_DIR`
//!    so `crate::theme`'s `include_bytes!` calls embed them at compile time.
//!    Archivo is a general-purpose external artifact, not whspr's own source,
//!    so the faces aren't vendored in git: `flake.nix` fetches them
//!    hermetically by content hash (`pkgs.fetchurl`) into a directory exposed
//!    as `ARCHIVO_DIR`, which this build script reads. This is the exact
//!    ARCHIVO_FACES copy loop from `whspr-app`'s `build.rs` (the installer
//!    reuses the app's font mechanism verbatim); it does *not* need the icon
//!    rasterization that build script also performs.
//!
//! 2. **Embedded app payload.** So the installer ships as a single
//!    self-contained `.exe`, it CONTAINS the app. When the oracle sets
//!    `WHSPR_APP_PAYLOAD_DIR` (at release-build time) to a staged app bundle
//!    (x64: `whspr-app.exe` + its 4 adjacent DLLs; arm64: just the exe), this
//!    enumerates that directory and generates `$OUT_DIR/payload_generated.rs`
//!    defining `pub const PAYLOAD: &[(&str, &[u8])]` via `include_bytes!`, so
//!    the SAME `crate::payload`/`crate::install` code produces the right
//!    per-arch installer. When it's UNSET -- the macOS gate, CI compile-checks,
//!    the screenshot harness -- an EMPTY payload is generated so the crate
//!    still builds everywhere.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The three static Archivo faces `crate::theme` embeds, by the filename
/// `ARCHIVO_DIR` (see `flake.nix`'s `archivoDir` derivation) holds each
/// one under.
const ARCHIVO_FACES: [&str; 3] = [
    "Archivo-Regular.ttf",
    "Archivo-SemiBold.ttf",
    "Archivo-ExtraBold.ttf",
];

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("cargo always sets OUT_DIR");
    let out_dir = Path::new(&out_dir);

    embed_asinvoker_manifest();
    copy_archivo_faces(out_dir);
    generate_payload(out_dir);
}

/// Embeds an `asInvoker` application manifest into the installer binary.
///
/// The installer is strictly **per-user** -- it copies into `%LOCALAPPDATA%\
/// whspr` and needs no administrator rights -- but its shipped filename
/// contains "setup", which trips Windows' *installer-detection heuristic*: a
/// GUI `.exe` named like `setup`/`install`/`update` with no application
/// manifest is assumed to be a legacy installer and auto-UAC-elevated on
/// launch. That elevation prompt is spurious and wrong here. An `asInvoker`
/// manifest tells Windows to run the program with the invoking user's own
/// authorities and never show the dialog, "regardless of the name of the
/// program".
///
/// Manifest embedding has to happen from `build.rs`, before the linker runs.
/// `embed-manifest`'s `new_manifest` already defaults the requested execution
/// level to `AsInvoker`; we set it explicitly so the intent is unmistakable.
/// The whole thing only does anything on a `windows-msvc` target (verified on
/// the Windows VM) -- the `CARGO_CFG_TARGET_OS` guard skips it entirely on the
/// macOS host build, so the crate stays green everywhere.
fn embed_asinvoker_manifest() {
    // `CARGO_CFG_TARGET_OS` reflects the *target* being built, not the host.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    use embed_manifest::manifest::ExecutionLevel;
    use embed_manifest::{embed_manifest, new_manifest};

    embed_manifest(
        new_manifest("Zevobla.whspr.Setup").requested_execution_level(ExecutionLevel::AsInvoker),
    )
    .expect("failed to embed asInvoker manifest");
}

/// Copies the Archivo faces from `ARCHIVO_DIR` into `OUT_DIR`.
fn copy_archivo_faces(out_dir: &Path) {
    let archivo_dir = env::var("ARCHIVO_DIR").unwrap_or_else(|_| {
        panic!(
            "ARCHIVO_DIR unset -- build inside `nix develop` (or otherwise \
             point ARCHIVO_DIR at a directory containing {})",
            ARCHIVO_FACES.join(", ")
        )
    });
    println!("cargo:rerun-if-env-changed=ARCHIVO_DIR");
    let archivo_dir = Path::new(&archivo_dir);

    for face in ARCHIVO_FACES {
        let src = archivo_dir.join(face);
        let dst = out_dir.join(face);
        println!("cargo:rerun-if-changed={}", src.display());
        // ARCHIVO_DIR's files are read-only Nix store paths, so a stale
        // read-only dst from a previous run can't be overwritten in place --
        // drop it first (removing a file only needs write access to the
        // directory, not the file itself).
        let _ = fs::remove_file(&dst);
        fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("failed to copy {} to {}: {e}", src.display(), dst.display()));
    }
}

/// Writes `$OUT_DIR/payload_generated.rs` -- see the module doc's job 2.
fn generate_payload(out_dir: &Path) {
    // Re-run whenever the payload directory is pointed somewhere else...
    println!("cargo:rerun-if-env-changed=WHSPR_APP_PAYLOAD_DIR");

    let dest = out_dir.join("payload_generated.rs");
    let entries = collect_payload();

    let mut src = String::from("// @generated by build.rs -- do not edit.\n");
    src.push_str("pub const PAYLOAD: &[(&str, &[u8])] = &[\n");
    for (name, abs_path) in &entries {
        src.push_str("    (");
        src.push_str(&rust_str_literal(name));
        src.push_str(", include_bytes!(");
        src.push_str(&rust_str_literal(&abs_path.to_string_lossy()));
        src.push_str(")),\n");
    }
    src.push_str("];\n");

    fs::write(&dest, src).expect("failed to write payload_generated.rs");
}

/// The `(file_name, absolute_path)` pairs to embed, or an empty vec when
/// `WHSPR_APP_PAYLOAD_DIR` is unset or points at nothing readable. Sorted by
/// name so the generated file is stable build-to-build.
fn collect_payload() -> Vec<(String, PathBuf)> {
    let Some(dir_os) = env::var_os("WHSPR_APP_PAYLOAD_DIR") else {
        return Vec::new();
    };

    // Resolve to an absolute path (relative to the crate root, cargo's build
    // cwd) so `include_bytes!` -- which resolves relative to the *generated*
    // file in OUT_DIR -- finds the files. Kept as a plain absolute path (not
    // `canonicalize`) so Windows never yields a `\\?\` verbatim path.
    let mut dir = PathBuf::from(&dir_os);
    if dir.is_relative() {
        if let Ok(cwd) = env::current_dir() {
            dir = cwd.join(dir);
        }
    }

    // ...and whenever a file appears/changes/disappears in it.
    println!("cargo:rerun-if-changed={}", dir.display());

    let Ok(read) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            entries.push((name.to_string(), path));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// Renders `s` as a double-quoted Rust string literal, escaping backslashes
/// and quotes -- staged Windows payload paths are full of `\`.
fn rust_str_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
