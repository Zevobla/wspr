//! whspr-refine statically links a Swift shim that weak-links the macOS-26
//! FoundationModels framework (see its build.rs). The framework search path
//! propagates from that crate, but the `-weak_framework` linker arg does not
//! reach this final binary — so re-emit it whenever the macOS-26 SDK is present
//! (the flake sets `WHSPR_MACOS26_SDK` exactly when the shim is built).
//! Weak-linking keeps the CLI loading on macOS 14-25.

fn main() {
    println!("cargo:rerun-if-env-changed=WHSPR_MACOS26_SDK");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var("WHSPR_MACOS26_SDK").is_ok()
    {
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
    }
}
