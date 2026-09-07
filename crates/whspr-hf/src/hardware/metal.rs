//! The macOS-only half of the GPU/unified-memory budget: the default Metal
//! device's `recommendedMaxWorkingSetSize`, the same signal LM Studio reads
//! on Apple Silicon to decide what a model can realistically use without
//! thrashing. Isolated in its own module (rather than inline `cfg` blocks in
//! [`super`]) so the non-macOS build of this crate never even sees the
//! `metal` crate -- see the stub below and `whspr-hf`'s `Cargo.toml`, which
//! only pulls `metal` in under `cfg(target_os = "macos")`.

/// The current machine's recommended Metal working-set size in bytes, or
/// `None` if this isn't macOS or no Metal device is present (e.g. running
/// under a sandbox/VM with no GPU). [`super::gpu_budget`] falls back to a
/// plain RAM fraction in either case.
#[cfg(target_os = "macos")]
pub fn recommended_working_set() -> Option<u64> {
    metal::Device::system_default().map(|device| device.recommended_max_working_set_size())
}

/// Off macOS there is no Metal device to ask, so this always defers to
/// [`super::gpu_budget`]'s RAM-fraction fallback.
#[cfg(not(target_os = "macos"))]
pub fn recommended_working_set() -> Option<u64> {
    None
}
