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

/// Pure fit verdict: compares a model's estimated peak footprint (see
/// [`estimated_footprint`]) against `available_bytes` of RAM.
///
/// - [`Fit::Green`]  when the footprint uses at most half of what's available
///   (so there's clear headroom),
/// - [`Fit::Yellow`] when it fits but uses more than half (tight),
/// - [`Fit::Red`]    when it exceeds available RAM outright.
///
/// Deterministic and side-effect-free so it can be unit-tested with injected
/// RAM values rather than whatever the test host happens to have.
pub fn fits(model_bytes: u64, available_bytes: u64) -> Fit {
    let footprint = estimated_footprint(model_bytes);
    if footprint.saturating_mul(2) <= available_bytes {
        Fit::Green
    } else if footprint <= available_bytes {
        Fit::Yellow
    } else {
        Fit::Red
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

    #[test]
    fn footprint_adds_a_third_of_overhead() {
        // 3 GB of weights -> 3 GB + 1 GB overhead = 4 GB.
        assert_eq!(estimated_footprint(3 * GB), 4 * GB);
    }

    #[test]
    fn green_when_footprint_at_most_half_of_available() {
        // 1 GB model -> ~1.33 GB footprint; 8 GB free -> plenty of headroom.
        assert_eq!(fits(GB, 8 * GB), Fit::Green);
    }

    #[test]
    fn yellow_when_fits_but_over_half() {
        // 3 GB model -> 4 GB footprint; 6 GB free: fits, but > half -> tight.
        assert_eq!(fits(3 * GB, 6 * GB), Fit::Yellow);
    }

    #[test]
    fn red_when_footprint_exceeds_available() {
        // 3 GB model -> 4 GB footprint; only 2 GB free -> won't load.
        assert_eq!(fits(3 * GB, 2 * GB), Fit::Red);
    }

    #[test]
    fn boundary_footprint_exactly_equals_available_is_yellow() {
        // A model whose footprint is exactly available RAM fits (barely).
        let model = 3 * GB; // footprint == 4 GB
        assert_eq!(fits(model, 4 * GB), Fit::Yellow);
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
