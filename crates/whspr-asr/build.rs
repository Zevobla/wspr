//! Compiles and links the Apple speech shim on macOS.
//!
//! `src/apple_speech.m` is a small Objective-C bridge to `Speech.framework`
//! (see `apple_speech.rs`). It's only built on macOS; every other target gets
//! an empty build script, so `cargo build --workspace` stays a pure-Rust build
//! off-mac.

fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/apple_speech.mm");
        // Compiled as Objective-C++ (.mm) on purpose. In an ObjC++ translation
        // unit, ObjC cleanups share the C++ exception personality
        // (`__gxx_personality_v0`) instead of emitting `___objc_personality_v0`.
        // The final binaries already link C++ (whisper/llama/sherpa), so that
        // personality is already present — reusing it keeps the shim from adding
        // a 4th distinct personality, which would overflow arm64 compact-unwind
        // ("too many personality routines for compact unwind").
        cc::Build::new()
            .cpp(true)
            .file("src/apple_speech.mm")
            .compile("whspr_apple_speech");
        // The frameworks the shim calls into (all present in the macOS SDK).
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Speech");
    }
}
