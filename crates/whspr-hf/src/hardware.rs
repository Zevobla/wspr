//! The "fits your machine" badge: a `sysinfo`-backed RAM probe plus a pure,
//! injected-input heuristic mapping a model's on-disk size against available
//! RAM to a red/yellow/green verdict (LM-Studio's model-list badge is the
//! reference).
//!
//! The verdict logic ([`fits`]) is deliberately split from the live probe
//! ([`probe`]): [`fits`] takes plain byte counts so it's a pure function
//! unit-testable with injected RAM values, while [`probe`] is the thin,
//! untested `sysinfo` shim that feeds it real numbers.

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

/// A snapshot of the host's RAM, in bytes. Kept as plain fields so callers
/// (and [`fits`]) never have to touch `sysinfo` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareSpecs {
    /// Total physical RAM installed.
    pub total_ram: u64,
    /// RAM the OS reports as currently available to new allocations.
    pub available_ram: u64,
}

impl HardwareSpecs {
    /// The fit verdict for a model of `model_bytes` on this machine, using
    /// [`available_ram`](Self::available_ram) as the budget.
    pub fn fit(&self, model_bytes: u64) -> Fit {
        fits(model_bytes, self.available_ram)
    }
}

/// Probes the host for its total and currently-available RAM via `sysinfo`.
/// The untested live counterpart to [`fits`]: refreshes only memory (not the
/// full, expensive system snapshot), so it's cheap enough to call on demand.
pub fn probe() -> HardwareSpecs {
    let mut system = System::new();
    system.refresh_memory();
    HardwareSpecs {
        total_ram: system.total_memory(),
        available_ram: system.available_memory(),
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
        };
        assert_eq!(specs.fit(GB), Fit::Red);
    }

    #[test]
    fn fit_labels_are_distinct() {
        assert_ne!(Fit::Green.label(), Fit::Yellow.label());
        assert_ne!(Fit::Yellow.label(), Fit::Red.label());
    }
}
