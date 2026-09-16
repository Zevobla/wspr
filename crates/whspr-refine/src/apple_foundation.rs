//! Apple Foundation Models refiner — on-device system LLM cleanup (macOS 26+).
//!
//! Thin Rust wrapper over the Swift shim in `apple_foundation.swift` (compiled
//! and linked by `build.rs`). The type and [`is_available`] exist in every
//! build so callers have a stable, `cfg`-free API; the actual FFI is gated on
//! `whspr_apple_fm` (set only when the shim was built). The framework is
//! weak-linked, so on macOS 14-25 [`is_available`] returns `false` and the
//! refiner is never selected; when the shim isn't built at all, `refine`
//! returns a clean "not available" error.

use async_trait::async_trait;
use whspr_core::{RefineContext, Result, TextRefiner, WhsprError};

#[cfg(whspr_apple_fm)]
use std::ffi::{CStr, CString};
#[cfg(whspr_apple_fm)]
use std::os::raw::c_char;

#[cfg(whspr_apple_fm)]
extern "C" {
    fn whspr_fm_available() -> i32;
    fn whspr_fm_refine(
        instructions: *const c_char,
        prompt: *const c_char,
        err_out: *mut *mut c_char,
    ) -> *mut c_char;
    fn whspr_fm_string_free(s: *mut c_char);
}

/// Whether Apple's on-device model is usable right now: the shim was built into
/// this binary AND the machine can run it (macOS 26+, Apple Intelligence on,
/// model present). `false` on older macOS (the framework is weak-linked and
/// absent there) or when the shim wasn't compiled — always safe to call.
pub fn is_available() -> bool {
    #[cfg(whspr_apple_fm)]
    {
        // SAFETY: the probe takes no arguments and only reads OS/model state.
        unsafe { whspr_fm_available() == 1 }
    }
    #[cfg(not(whspr_apple_fm))]
    {
        false
    }
}

/// On-device cleanup via Apple's Foundation Models system LLM (macOS 26+).
/// Nothing leaves the machine; there is no model to download.
#[derive(Default)]
pub struct AppleFoundation {
    shorten: bool,
}

impl AppleFoundation {
    pub fn new() -> Self {
        Self::default()
    }

    /// J-11: append the concise-mode instruction to the cleanup prompt.
    /// Off by default; whspr-cli wires this from `[capture].shorten`.
    pub fn with_shorten(mut self, shorten: bool) -> Self {
        self.shorten = shorten;
        self
    }
}

/// Calls the shim on the current (blocking) thread, marshalling its C strings.
#[cfg(whspr_apple_fm)]
fn refine_blocking(instructions: &str, prompt: &str) -> Result<String> {
    let instr = CString::new(instructions)
        .map_err(|_| WhsprError::Refine("instructions contained a NUL byte".into()))?;
    let prm = CString::new(prompt)
        .map_err(|_| WhsprError::Refine("prompt contained a NUL byte".into()))?;

    let mut err: *mut c_char = std::ptr::null_mut();
    // SAFETY: both C strings outlive the call and are only read by the shim,
    // which returns a malloc'd result or NULL plus a malloc'd *err — both freed
    // below before returning.
    let out = unsafe { whspr_fm_refine(instr.as_ptr(), prm.as_ptr(), &mut err) };

    if !out.is_null() {
        // SAFETY: `out` is a valid NUL-terminated UTF-8 string from the shim.
        let s = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { whspr_fm_string_free(out) };
        return Ok(s.trim().to_string());
    }
    let msg = if err.is_null() {
        "Foundation Models refine failed".to_string()
    } else {
        // SAFETY: a non-null `err` is a valid string from the shim.
        let m = unsafe { CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned();
        unsafe { whspr_fm_string_free(err) };
        m
    };
    Err(WhsprError::Refine(msg))
}

#[async_trait]
impl TextRefiner for AppleFoundation {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        #[cfg(whspr_apple_fm)]
        {
            // The full cleanup rules live in the prompt (as with the other
            // backends); the session gets a short system instruction on top.
            let instructions =
                "You clean up raw speech-to-text. Output only the cleaned text, with no preamble.";
            let prompt = crate::build_cleanup_prompt(raw, ctx, self.shorten);
            // Foundation Models inference is synchronous from our side (the shim
            // blocks); keep it off the async runtime, like the llama-local path.
            tokio::task::spawn_blocking(move || refine_blocking(instructions, &prompt))
                .await
                .map_err(|e| WhsprError::Refine(format!("apple-foundation task panicked: {e}")))?
        }
        #[cfg(not(whspr_apple_fm))]
        {
            let _ = (raw, ctx, self.shorten);
            Err(WhsprError::Refine(
                "Apple Foundation Models was not built into this binary".into(),
            ))
        }
    }

    fn id(&self) -> &'static str {
        "apple-foundation"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_apple_foundation() {
        assert_eq!(AppleFoundation::new().id(), "apple-foundation");
    }

    #[test]
    fn availability_probe_links_and_does_not_crash() {
        // When the shim is built this calls across the C ABI into Swift; on
        // macOS < 26 the weak framework is absent and it must return false
        // (never crash) — proving the shim compiled, linked, and loads.
        let _ = is_available();
    }

    #[tokio::test]
    async fn refine_when_unavailable_errors_without_panicking() {
        // On a machine/build without the model, refine must surface a clean
        // error rather than panicking.
        if is_available() {
            return;
        }
        let result = AppleFoundation::new()
            .refine("um hello there", &RefineContext::default())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn with_shorten_does_not_disturb_the_unavailable_error_path() {
        // Same reasoning as refine_when_unavailable_errors_without_panicking:
        // no real device/model in this test environment to inspect the
        // actual prompt against (that's build_cleanup_prompt's own tests,
        // in lib.rs), so this just proves the builder wires through cleanly.
        if is_available() {
            return;
        }
        let result = AppleFoundation::new()
            .with_shorten(true)
            .refine("um hello there", &RefineContext::default())
            .await;
        assert!(result.is_err());
    }
}
