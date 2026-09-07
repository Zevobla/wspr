//! The "fits your machine" badge: a `sysinfo`-backed RAM probe plus a pure,
//! injected-input heuristic mapping a model's on-disk size against available
//! RAM to a red/yellow/green verdict (LM-Studio's model-list badge is the
//! reference).
//!
//! The verdict logic ([`fits`]) is deliberately split from the live probe
//! ([`probe`]): [`fits`] takes plain byte counts so it's a pure function
//! unit-testable with injected RAM values, while [`probe`] is the thin,
//! untested `sysinfo`/Metal shim that feeds it real numbers.

mod metal;

use sysinfo::System;

/// How comfortably a model is expected to fit in a machine's RAM. Mirrors
/// LM-Studio's three-state model badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Loads with comfortable headroom to spare.
    Green,
    /// Fits, but leaves little room -- expect memory pressure.
    Yellow,
    /// The estimated footprint exceeds available RAM; likely won't load.
    Red,
}

impl Fit {
    /// A short human label for the badge (e.g. next to a model in the list).
    pub fn label(self) -> &'static str {
        match self {
            Fit::Green => "Fits your machine",
            Fit::Yellow => "Tight fit",
            Fit::Red => "Too large",
        }
    }
}

/// Runtime memory overhead whisper.cpp needs on top of the raw model weights
/// (KV cache + compute buffers), as a fraction of the model size. A coarse
/// rule of thumb, not a measured constant -- the heuristic only needs to be
/// good enough to color a badge, and the split between [`fits`] and [`probe`]
/// keeps it honest and testable.
const RUNTIME_OVERHEAD_NUMERATOR: u64 = 1;
const RUNTIME_OVERHEAD_DENOMINATOR: u64 = 3;

/// Estimated peak RAM a model of `model_bytes` needs to load and run: the
/// weights plus [`RUNTIME_OVERHEAD_NUMERATOR`]/[`RUNTIME_OVERHEAD_DENOMINATOR`]
/// of that again for runtime buffers. Saturating so an absurd size can't wrap.
pub fn estimated_footprint(model_bytes: u64) -> u64 {
    let overhead = model_bytes
        .saturating_mul(RUNTIME_OVERHEAD_NUMERATOR)
        .saturating_div(RUNTIME_OVERHEAD_DENOMINATOR);
    model_bytes.saturating_add(overhead)
}

/// RAM to keep free for the OS, this app, and whatever else the user is
/// running. A model is only ever "fits" if loading it still leaves at least
/// this much headroom -- a raw `footprint <= available` check reads far too
/// generously (it would flag a 3 GB model on an 8 GB machine as fine).
const OS_RESERVE_BYTES: u64 = 1536 * 1024 * 1024; // 1.5 GiB

/// For a comfortable ([`Fit::Green`]) verdict a model's footprint must use at
/// most this fraction (1/N) of available RAM, so most of the machine stays
/// free. Small models clear this easily; a mid/large whisper model on ~8 GB
/// does not, so it reads [`Fit::Yellow`] (tight) rather than green.
const GREEN_HEADROOM_DIVISOR: u64 = 5;

/// Pure fit verdict: compares a model's estimated peak footprint (see
/// [`estimated_footprint`]) against `available_bytes` of RAM, reserving
/// [`OS_RESERVE_BYTES`] for everything else.
///
/// - [`Fit::Red`]    when loading it would leave less than the OS reserve
///   free (it would not practically load),
/// - [`Fit::Green`]  when the footprint uses at most 1/[`GREEN_HEADROOM_DIVISOR`]
///   of available RAM (comfortable headroom),
/// - [`Fit::Yellow`] otherwise (it fits with the reserve, but is tight).
///
/// Deterministic and side-effect-free so it can be unit-tested with injected
/// RAM values rather than whatever the test host happens to have.
pub fn fits(model_bytes: u64, available_bytes: u64) -> Fit {
    let footprint = estimated_footprint(model_bytes);
    if footprint.saturating_add(OS_RESERVE_BYTES) > available_bytes {
        Fit::Red
    } else if footprint.saturating_mul(GREEN_HEADROOM_DIVISOR) <= available_bytes {
        Fit::Green
    } else {
        Fit::Yellow
    }
}

/// LM-Studio's default unified-memory fraction: how much of total physical
/// RAM is assumed usable as GPU working set when there's no better signal
/// (off macOS, or no Metal device found).
const UNIFIED_MEMORY_FRACTION_NUMERATOR: u64 = 7;
const UNIFIED_MEMORY_FRACTION_DENOMINATOR: u64 = 10;

/// The plain RAM-fraction fallback ([`UNIFIED_MEMORY_FRACTION_NUMERATOR`]/
/// [`UNIFIED_MEMORY_FRACTION_DENOMINATOR`] of `total_ram`) used when no
/// Metal working-set figure is available. Pure and unit-tested on its own so
/// [`gpu_budget`]'s live Metal call doesn't have to be exercised to cover it.
fn unified_memory_fraction(total_ram: u64) -> u64 {
    total_ram
        .saturating_mul(UNIFIED_MEMORY_FRACTION_NUMERATOR)
        .saturating_div(UNIFIED_MEMORY_FRACTION_DENOMINATOR)
}

/// The usable GPU/unified-memory budget for this machine: the Metal default
/// device's `recommendedMaxWorkingSetSize` on Apple Silicon (mirrors LM
/// Studio -- see [`metal::recommended_working_set`]), or, off macOS or when
/// no Metal device is found, [`unified_memory_fraction`] of `total_ram`.
pub fn gpu_budget(total_ram: u64) -> u64 {
    metal::recommended_working_set().unwrap_or_else(|| unified_memory_fraction(total_ram))
}

/// The tighter of the GPU/unified-memory budget and the currently-available
/// RAM once [`OS_RESERVE_BYTES`] is set aside for the OS, this app, and
/// anything else running -- a raw `min(gpu_budget, available_ram)` would let
/// a machine under memory pressure read as fine just because its GPU budget
/// is large. Pure and unit-tested with injected values; not yet consumed by
/// [`fits`] (see the follow-up commit that reworks the verdict around it).
pub fn usable_memory(gpu_budget: u64, available_ram: u64) -> u64 {
    gpu_budget.min(available_ram.saturating_sub(OS_RESERVE_BYTES))
}

/// A snapshot of the host's RAM, in bytes. Kept as plain fields so callers
/// (and [`fits`]) never have to touch `sysinfo`/Metal directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareSpecs {
    /// Total physical RAM installed.
    pub total_ram: u64,
    /// RAM the OS reports as currently available to new allocations.
    pub available_ram: u64,
    /// The usable GPU/unified-memory budget (see [`usable_memory`]) --
    /// already the tighter of [`gpu_budget`] and
    /// [`available_ram`](Self::available_ram) less the OS reserve.
    pub usable_ram: u64,
}

impl HardwareSpecs {
    /// The fit verdict for a model of `model_bytes` on this machine, using
    /// [`available_ram`](Self::available_ram) as the budget.
    pub fn fit(&self, model_bytes: u64) -> Fit {
        fits(model_bytes, self.available_ram)
    }
}

/// Probes the host for its total/available RAM via `sysinfo` and its
/// GPU/unified-memory budget via [`gpu_budget`], then derives
/// [`HardwareSpecs::usable_ram`] via [`usable_memory`]. The untested live
/// counterpart to [`fits`]: refreshes only memory (not the full, expensive
/// system snapshot), so it's cheap enough to call on demand.
pub fn probe() -> HardwareSpecs {
    let mut system = System::new();
    system.refresh_memory();
    let total_ram = system.total_memory();
    let available_ram = system.available_memory();
    let budget = gpu_budget(total_ram);
    HardwareSpecs {
        total_ram,
        available_ram,
        usable_ram: usable_memory(budget, available_ram),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    #[test]
    fn footprint_adds_a_third_of_overhead() {
        // 3 GB of weights -> 3 GB + 1 GB overhead = 4 GB.
        assert_eq!(estimated_footprint(3 * GB), 4 * GB);
    }

    #[test]
    fn small_models_are_green_on_an_8gb_machine() {
        // tiny / base / small (whisper) all leave most of an 8 GB machine
        // free, so they read comfortably green.
        assert_eq!(fits(75 * MB, 8 * GB), Fit::Green);
        assert_eq!(fits(142 * MB, 8 * GB), Fit::Green);
        assert_eq!(fits(466 * MB, 8 * GB), Fit::Green);
    }

    #[test]
    fn medium_and_large_turbo_are_tight_on_8gb() {
        // ~1.5-1.6 GB weights -> ~2.0-2.1 GB footprint: fits with the OS
        // reserve, but uses well over a fifth of 8 GB -> tight, not green.
        assert_eq!(fits(1500 * MB, 8 * GB), Fit::Yellow);
        assert_eq!(fits(1600 * MB, 8 * GB), Fit::Yellow);
    }

    #[test]
    fn large_v3_is_tight_not_green_on_8gb() {
        // The regression this retune fixes: 3 GB weights -> 4 GB footprint on
        // an ~8 GB machine must read Yellow (tight), never green.
        assert_eq!(fits(3 * GB, 8 * GB), Fit::Yellow);
    }

    #[test]
    fn red_when_footprint_exceeds_available() {
        // 3 GB model -> 4 GB footprint; only 2 GB free -> won't load.
        assert_eq!(fits(3 * GB, 2 * GB), Fit::Red);
    }

    #[test]
    fn red_when_loading_would_starve_the_os_reserve() {
        // 3.5 GB weights -> ~4.67 GB footprint fits raw under 5 GB, but the
        // 1.5 GB OS reserve pushes it over -> Red.
        assert_eq!(fits(3500 * MB, 5 * GB), Fit::Red);
    }

    #[test]
    fn zero_available_ram_is_always_red() {
        assert_eq!(fits(GB, 0), Fit::Red);
    }

    #[test]
    fn specs_fit_uses_available_not_total() {
        // Lots of total RAM but almost none available -> Red, proving the
        // budget is `available_ram`, not `total_ram`.
        let specs = HardwareSpecs {
            total_ram: 64 * GB,
            available_ram: GB / 2,
            usable_ram: 0,
        };
        assert_eq!(specs.fit(GB), Fit::Red);
    }

    #[test]
    fn fit_labels_are_distinct() {
        assert_ne!(Fit::Green.label(), Fit::Yellow.label());
        assert_ne!(Fit::Yellow.label(), Fit::Red.label());
    }

    #[test]
    fn unified_memory_fraction_is_seventy_percent_of_total() {
        assert_eq!(unified_memory_fraction(10 * GB), 7 * GB);
    }

    #[test]
    fn usable_memory_is_the_tighter_of_budget_and_reserved_available() {
        // Plenty of GPU budget, but little available RAM once the OS
        // reserve is set aside -> available wins (and is the smaller side).
        assert_eq!(usable_memory(64 * GB, 2 * GB), (2 * GB) - OS_RESERVE_BYTES);

        // Lots of available RAM, but a small GPU budget -> budget wins.
        assert_eq!(usable_memory(3 * GB, 64 * GB), 3 * GB);
    }

    #[test]
    fn usable_memory_saturates_when_reserve_exceeds_available() {
        // Less available RAM than the OS reserve alone -> zero, not a wrap.
        assert_eq!(usable_memory(64 * GB, GB / 2), 0);
    }
}
