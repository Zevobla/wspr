//! Compiles the Apple Foundation Models Swift shim (`src/apple_foundation.swift`)
//! on macOS, when a Swift 6.3+ toolchain and the macOS-26 SDK are provided via
//! `WHSPR_SWIFTC` / `WHSPR_MACOS26_SDK` (set by flake.nix). Without them the shim
//! is skipped and the `AppleFoundation` refiner compiles out (`cfg(whspr_apple_fm)`
//! is not set), so every other target — and any macOS build without the SDK —
//! still builds.
//!
//! The shim is emitted as an object and archived into a static lib, so it links
//! straight into the final binary with no runtime dylib to relocate. Its Swift
//! runtime deps resolve to /usr/lib/swift (in the OS since 10.14.4); the only
//! macOS-26 dependency, FoundationModels, is weak-linked (the `-weak_framework`
//! is re-emitted by the binary crates), so the app still loads on macOS 14-25.

use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(whspr_apple_fm)");
    println!("cargo:rerun-if-env-changed=WHSPR_SWIFTC");
    println!("cargo:rerun-if-env-changed=WHSPR_MACOS26_SDK");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let (Ok(swiftc), Ok(sdk)) = (
        std::env::var("WHSPR_SWIFTC"),
        std::env::var("WHSPR_MACOS26_SDK"),
    ) else {
        println!(
            "cargo:warning=WHSPR_SWIFTC/WHSPR_MACOS26_SDK unset; \
             Apple Foundation Models refiner disabled for this build"
        );
        return;
    };

    let src = "src/apple_foundation.swift";
    println!("cargo:rerun-if-changed={src}");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let swift_arch = if arch == "x86_64" { "x86_64" } else { "arm64" };
    // Deployment target 14.0 while building against the 26 SDK: the app keeps
    // its macOS-14 floor and the framework simply lights up on 26+.
    let target = format!("{swift_arch}-apple-macos14.0");
    let obj = format!("{out_dir}/apple_foundation.o");
    let lib = format!("{out_dir}/libwhspr_fm.a");

    let status = Command::new(&swiftc)
        .args([
            "-sdk", &sdk,
            "-target", &target,
            "-swift-version", "6",
            "-O",
            "-parse-as-library",
            "-emit-object",
            "-o", &obj,
            // Don't autolink FoundationModels strongly — the binary crates add
            // it as `-weak_framework` so the app still loads pre-macOS-26.
            "-Xfrontend", "-disable-autolink-framework", "-Xfrontend", "FoundationModels",
            src,
        ])
        .status()
        .expect("failed to spawn swiftc");
    assert!(status.success(), "swiftc failed to compile the Foundation Models shim");

    let ar = std::process::Command::new("ar")
        .args(["crs", &lib, &obj])
        .status()
        .expect("failed to spawn ar");
    assert!(ar.success(), "ar failed to archive the Foundation Models shim");

    // The shim's static archive.
    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=whspr_fm");
    // The Swift object references the Objective-C runtime (objc_alloc, ...) for
    // ObjC interop; link libobjc so those imports resolve.
    println!("cargo:rustc-link-lib=dylib=objc");
    // Swift runtime import libraries (their install-names point at the OS copies
    // under /usr/lib/swift, present since macOS 10.14.4 — nothing is bundled).
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    // The macOS-26 SDK's frameworks (for FoundationModels); this search path
    // propagates to the binary crates' final link too.
    println!("cargo:rustc-link-search=framework={sdk}/System/Library/Frameworks");
    // FoundationModels is macOS-26-only: weak-link it so the binary still loads
    // on 14-25. This link-arg reaches whspr-refine's own test binary; the app/cli
    // binaries re-emit it from their own build.rs (a dependency's link-arg doesn't
    // reach the final link) — they gate on the same WHSPR_MACOS26_SDK env var.
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");

    println!("cargo:rustc-cfg=whspr_apple_fm");
}
