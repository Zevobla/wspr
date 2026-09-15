//! The Windows-only half of the GPU budget: the primary discrete GPU's DXGI
//! `DedicatedVideoMemory`, the Windows analogue of macOS's Metal
//! working-set probe (see `metal.rs`) -- the signal that lets the "fits your
//! machine" badge size a model against real VRAM rather than bare system RAM
//! on a discrete-GPU box. Isolated in its own module (rather than inline `cfg`
//! blocks in [`super`]) so the non-Windows build of this crate never even sees
//! the `windows` crate -- see the stub below and `whspr-hf`'s `Cargo.toml`,
//! which only pulls `windows` in under `cfg(target_os = "windows")`.

/// The largest `DXGI_ADAPTER_DESC::DedicatedVideoMemory` across the machine's
/// DXGI adapters, in bytes, or `None` if this isn't Windows, DXGI can't be
/// queried, or no adapter reports any dedicated VRAM (the Microsoft Basic
/// Render / WARP software adapter reports zero, so the `max` naturally skips
/// it and an integrated-only box falls through). [`super::gpu_budget`] then
/// falls back to a plain RAM fraction.
#[cfg(target_os = "windows")]
pub fn dedicated_vram_bytes() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    // All of the DXGI/COM calls below are FFI: create the factory, walk the
    // adapters until `EnumAdapters` reports there are no more, and read each
    // descriptor. Every failure path returns `None` (via `ok()?` or by
    // skipping the adapter) rather than panicking.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: u64 = 0;
        let mut index: u32 = 0;
        while let Ok(adapter) = factory.EnumAdapters(index) {
            if let Ok(desc) = adapter.GetDesc() {
                // `DedicatedVideoMemory` is a `usize`; widen it to the `u64`
                // the budget math works in.
                best = best.max(desc.DedicatedVideoMemory as u64);
            }
            index += 1;
        }
        (best > 0).then_some(best)
    }
}

/// Off Windows there is no DXGI adapter to ask, so this always defers to
/// [`super::gpu_budget`]'s RAM-fraction fallback.
#[cfg(not(target_os = "windows"))]
pub fn dedicated_vram_bytes() -> Option<u64> {
    None
}
