//! The "fits your machine" badge: a GPU/unified-memory budget probe plus a
//! pure, injected-input heuristic mapping a model's estimated footprint
//! against that budget to a red/yellow/green verdict (LM-Studio's model-list
//! badge is the reference, both in the traffic-light badge and in what the
//! budget is judged against -- on Apple Silicon models run against Metal's
//! unified memory, not bare system RAM).
//!
//! The verdict logic ([`fits`], [`estimated_footprint`], [`usable_memory`])
//! is deliberately split from the live probes ([`probe`], [`gpu_budget`],
//! [`metal::recommended_working_set`]): the former are pure functions
//! unit-testable with injected byte counts, while the latter are the thin,
//! untested `sysinfo`/Metal shims that feed them real numbers.
//!
//! [`estimated_footprint`]/[`fits`] are the coarse, size-only estimators (a
//! byte count plus a [`ModelKind`], nothing more) used when a model's
//! specific architecture isn't known. When it is -- the curated LLM registry
//! ([`crate::models::LlmModel`]) or a GGUF file whose header
//! ([`crate::gguf`]) parsed successfully -- [`kv_cache_bytes`],
//! [`estimated_llm_footprint`] and [`fits_llm`] compute the exact
//! KV-cache-aware figure instead, from the model's real
//! layer/embedding/head counts and context length.

mod metal;

use sysinfo::System;

use crate::models::ModelKind;

/// How comfortably a model is expected to fit in a machine's usable
/// GPU/unified-memory budget. Mirrors LM-Studio's three-state model badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Loads with comfortable headroom to spare.
    Green,
    /// Fits, but leaves little room -- expect memory pressure.
    Yellow,
    /// The estimated footprint exceeds the usable budget; likely won't load.
    Red,
}

impl Fit {
    /// A short human label for the badge (e.g. next to a model in the list).
    pub fn label(self) -> &'static str {
        match self {
            Fit::Green => "Fits your machine",
            Fit::Yellow => "Tight fit",
            Fit::Red => "Won't fit",
        }
    }
}

/// One gibibyte, as a byte count -- the unit every constant below is
/// expressed in.
const GIB: u64 = 1024 * 1024 * 1024;

/// whisper.cpp pre-allocates a fixed working set (KV cache + compute
/// buffers) rather than scaling it with context length, so its overhead is a
/// weight-proportional term plus one small fixed pad rather than a separate
/// context-dependent one.
const ASR_WEIGHT_NUMERATOR: u64 = 13;
const ASR_WEIGHT_DENOMINATOR: u64 = 10;
/// The fixed pad on top of the weight-proportional term above.
const ASR_FIXED_OVERHEAD_BYTES: u64 = 3 * GIB / 10; // 0.3 GiB

/// A modest, size-independent KV-cache estimate for a llama.cpp GGUF at a
/// default context length. This is the coarse fallback [`estimated_footprint`]
/// uses when only a byte count and a [`ModelKind`] are available (no
/// specific model identity) -- when the model's real architecture is known,
/// [`estimated_llm_footprint`] computes the exact figure from
/// [`kv_cache_bytes`] instead; see [`crate::models::LlmModel::estimated_footprint`]
/// (the curated registry) and [`crate::gguf::estimated_footprint_for_file`]
/// (a scanned local file).
const LLM_KV_ESTIMATE_BYTES: u64 = 4 * GIB / 10; // 0.4 GiB
/// Weights-adjacent compute-buffer overhead on top of the KV estimate above.
const LLM_FIXED_OVERHEAD_BYTES: u64 = 7 * GIB / 10; // 0.7 GiB

/// Estimated peak memory a model of `model_bytes` needs to load and run,
/// using [`ModelKind`]-specific math: whisper GGML pre-allocates a fixed
/// working set ([`ASR_WEIGHT_NUMERATOR`]/[`ASR_WEIGHT_DENOMINATOR`] of the
/// weights, plus [`ASR_FIXED_OVERHEAD_BYTES`]), while a llama.cpp GGUF adds a
/// KV-cache estimate and its own fixed overhead
/// ([`LLM_KV_ESTIMATE_BYTES`] + [`LLM_FIXED_OVERHEAD_BYTES`]). Saturating so
/// an absurd size can't wrap.
pub fn estimated_footprint(model_bytes: u64, kind: ModelKind) -> u64 {
    match kind {
        ModelKind::Asr => model_bytes
            .saturating_mul(ASR_WEIGHT_NUMERATOR)
            .saturating_div(ASR_WEIGHT_DENOMINATOR)
            .saturating_add(ASR_FIXED_OVERHEAD_BYTES),
        ModelKind::Llm => model_bytes
            .saturating_add(LLM_KV_ESTIMATE_BYTES)
            .saturating_add(LLM_FIXED_OVERHEAD_BYTES),
    }
}

/// For a comfortable ([`Fit::Green`]) verdict a model's footprint must use at
/// most this fraction of the usable budget, so most of it stays free.
const GREEN_BUDGET_NUMERATOR: u64 = 60;
const GREEN_BUDGET_DENOMINATOR: u64 = 100;
/// Above [`Fit::Green`] but at or under this fraction of the usable budget
/// still loads, just tight ([`Fit::Yellow`]); beyond it reads [`Fit::Red`].
const YELLOW_BUDGET_NUMERATOR: u64 = 90;
const YELLOW_BUDGET_DENOMINATOR: u64 = 100;

/// Pure fit verdict: compares a model's estimated peak footprint (see
/// [`estimated_footprint`]) against `usable_bytes` -- the machine's usable
/// GPU/unified-memory budget (see [`usable_memory`]), which already has the
/// OS/app reserve subtracted out.
///
/// - [`Fit::Green`]  when the footprint uses at most
///   [`GREEN_BUDGET_NUMERATOR`]/[`GREEN_BUDGET_DENOMINATOR`] of the budget
///   (comfortable headroom),
/// - [`Fit::Yellow`] when it's over that but still at most
///   [`YELLOW_BUDGET_NUMERATOR`]/[`YELLOW_BUDGET_DENOMINATOR`] (fits, tight),
/// - [`Fit::Red`]    otherwise (would not practically load).
///
/// Deterministic and side-effect-free so it can be unit-tested with injected
/// budget values rather than whatever the test host happens to have.
pub fn fits(model_bytes: u64, usable_bytes: u64, kind: ModelKind) -> Fit {
    verdict(estimated_footprint(model_bytes, kind), usable_bytes)
}

/// The shared green/yellow/red threshold comparison behind both [`fits`] and
/// [`fits_llm`] -- factored out so the exact, GGUF-metadata-based LLM path
/// judges its footprint against exactly the same budget fractions as the
/// coarse, size-only one.
fn verdict(footprint: u64, usable_bytes: u64) -> Fit {
    let green_budget = usable_bytes
        .saturating_mul(GREEN_BUDGET_NUMERATOR)
        .saturating_div(GREEN_BUDGET_DENOMINATOR);
    let yellow_budget = usable_bytes
        .saturating_mul(YELLOW_BUDGET_NUMERATOR)
        .saturating_div(YELLOW_BUDGET_DENOMINATOR);
    if footprint <= green_budget {
        Fit::Green
    } else if footprint <= yellow_budget {
        Fit::Yellow
    } else {
        Fit::Red
    }
}

/// A llama.cpp GGUF's architecture shape -- exactly what [`kv_cache_bytes`]
/// needs to compute an exact KV-cache estimate, whether baked into the
/// curated registry ([`crate::models::LlmModel`]) or read from a local
/// file's GGUF header ([`crate::gguf::GgufMetadata`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmShape {
    /// Transformer block/layer count (GGUF `<arch>.block_count`).
    pub n_layers: u64,
    /// Hidden/embedding size (GGUF `<arch>.embedding_length`).
    pub n_embd: u64,
    /// Attention head count (GGUF `<arch>.attention.head_count`).
    pub n_head: u64,
    /// KV head count -- equal to [`Self::n_head`] for ordinary multi-head
    /// attention, smaller under grouped-query attention (GQA) (GGUF
    /// `<arch>.attention.head_count_kv`).
    pub n_head_kv: u64,
    /// The model's trained/released context length (GGUF
    /// `<arch>.context_length`), *before* the [`DEFAULT_WORKING_CONTEXT`]
    /// cap [`kv_cache_bytes`] applies.
    pub context_length: u64,
}

/// f16 KV cache: 2 bytes per cached element (one `K` and one `V` per token
/// per KV head).
const KV_BYTES_PER_ELEM: u64 = 2;

/// The working context assumed for the exact KV-cache estimate: a model's
/// trained context capped at this sane default. A local text-refiner pass
/// over one dictated utterance never remotely approaches a 32K/128K trained
/// context, so pricing the KV cache at the full trained context would wildly
/// overstate real memory use.
const DEFAULT_WORKING_CONTEXT: u64 = 4096;

/// Fixed compute-buffer/overhead pad added on top of weights + KV cache for
/// the exact, GGUF-metadata-based footprint (see [`estimated_llm_footprint`]).
/// Smaller than the coarse path's [`LLM_FIXED_OVERHEAD_BYTES`] because the
/// exact path already prices the KV cache itself precisely rather than
/// folding a margin for it into this pad.
const LLM_EXACT_OVERHEAD_BYTES: u64 = GIB / 2; // 0.5 GiB

/// The exact llama.cpp KV-cache size for `shape` at a working context of
/// `min(shape.context_length, DEFAULT_WORKING_CONTEXT)`:
/// `2 (K and V) * n_layers * n_ctx * n_embd_kv * bytes_per_elem`, where
/// `n_embd_kv = head_dim * n_head_kv` (GQA-aware: `head_dim = n_embd /
/// n_head`, so `n_embd_kv` shrinks below `n_embd` exactly when
/// `n_head_kv < n_head`, and equals it for ordinary multi-head attention).
/// Saturating throughout so a malformed/adversarial shape can't wrap.
pub fn kv_cache_bytes(shape: LlmShape) -> u64 {
    let n_ctx = shape.context_length.min(DEFAULT_WORKING_CONTEXT);
    let head_dim = shape.n_embd.checked_div(shape.n_head).unwrap_or(0);
    let n_embd_kv = head_dim.saturating_mul(shape.n_head_kv);
    2u64.saturating_mul(shape.n_layers)
        .saturating_mul(n_ctx)
        .saturating_mul(n_embd_kv)
        .saturating_mul(KV_BYTES_PER_ELEM)
}

/// Exact, GGUF-metadata-based footprint for an LLM of `model_bytes` weights
/// and architecture `shape`: weights + [`kv_cache_bytes`] +
/// [`LLM_EXACT_OVERHEAD_BYTES`]. The metadata-aware counterpart to
/// [`estimated_footprint`]'s coarse, size-only [`ModelKind::Llm`] branch --
/// used wherever a model's real architecture is known (the curated registry
/// or a parsed local GGUF header).
pub fn estimated_llm_footprint(model_bytes: u64, shape: LlmShape) -> u64 {
    model_bytes
        .saturating_add(kv_cache_bytes(shape))
        .saturating_add(LLM_EXACT_OVERHEAD_BYTES)
}

/// The [`Fit`] verdict computed from the exact footprint
/// ([`estimated_llm_footprint`]) against `usable_bytes`, using the same
/// green/yellow/red thresholds as [`fits`] (see [`verdict`]).
pub fn fits_llm(model_bytes: u64, shape: LlmShape, usable_bytes: u64) -> Fit {
    verdict(estimated_llm_footprint(model_bytes, shape), usable_bytes)
}

/// RAM to keep free for the OS, this app, and whatever else the user is
/// running -- subtracted from available RAM before it ever reaches
/// [`usable_memory`]'s `min` against the GPU budget.
const OS_RESERVE_BYTES: u64 = 3 * GIB / 2; // 1.5 GiB

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

/// The budget [`fits`] actually judges a model against: the tighter of the
/// GPU/unified-memory budget and the currently-available RAM once
/// [`OS_RESERVE_BYTES`] is set aside for the OS, this app, and anything else
/// running -- a raw `min(gpu_budget, available_ram)` would let a machine
/// under memory pressure read as fine just because its GPU budget is large.
/// Pure and unit-tested with injected values.
pub fn usable_memory(gpu_budget: u64, available_ram: u64) -> u64 {
    gpu_budget.min(available_ram.saturating_sub(OS_RESERVE_BYTES))
}

/// A snapshot of the host's memory budget, in bytes. Kept as plain fields so
/// callers (and [`fits`]) never have to touch `sysinfo`/Metal directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareSpecs {
    /// Total physical RAM installed.
    pub total_ram: u64,
    /// RAM the OS reports as currently available to new allocations.
    pub available_ram: u64,
    /// The usable GPU/unified-memory budget models are judged against (see
    /// [`usable_memory`]) -- already the tighter of [`gpu_budget`] and
    /// [`available_ram`](Self::available_ram) less the OS reserve.
    pub usable_ram: u64,
}

impl HardwareSpecs {
    /// The fit verdict for a model of `model_bytes` of kind `kind` on this
    /// machine, using [`usable_ram`](Self::usable_ram) as the budget.
    pub fn fit(&self, model_bytes: u64, kind: ModelKind) -> Fit {
        fits(model_bytes, self.usable_ram, kind)
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
    fn asr_footprint_scales_weights_and_adds_a_fixed_pad() {
        // 3 GB of weights -> 3.9 GB (x1.3) + 0.3 GB pad = 4.2 GB.
        assert_eq!(
            estimated_footprint(3 * GB, ModelKind::Asr),
            (3 * GB * 13 / 10) + ASR_FIXED_OVERHEAD_BYTES
        );
    }

    #[test]
    fn llm_footprint_adds_kv_and_fixed_overhead() {
        // 2 GB of weights -> 2 GB + 0.4 GB KV + 0.7 GB overhead = 3.1 GB.
        assert_eq!(
            estimated_footprint(2 * GB, ModelKind::Llm),
            2 * GB + LLM_KV_ESTIMATE_BYTES + LLM_FIXED_OVERHEAD_BYTES
        );
    }

    #[test]
    fn small_whisper_models_are_green_on_a_3gb_usable_budget() {
        // tiny / base / small (whisper) all leave most of a modest 3 GB
        // usable budget free, so they read comfortably green.
        let usable = 3 * GB;
        assert_eq!(fits(75 * MB, usable, ModelKind::Asr), Fit::Green);
        assert_eq!(fits(142 * MB, usable, ModelKind::Asr), Fit::Green);
        assert_eq!(fits(466 * MB, usable, ModelKind::Asr), Fit::Green);
    }

    #[test]
    fn medium_and_large_turbo_whisper_are_tight_on_a_3gb_usable_budget() {
        // ~1.5-1.6 GB weights -> ~2.2-2.4 GB footprint on a 3 GB usable
        // budget: over the 60% (1.8 GB) green line but under the 90%
        // (2.7 GB) yellow one.
        let usable = 3 * GB;
        assert_eq!(fits(1500 * MB, usable, ModelKind::Asr), Fit::Yellow);
        assert_eq!(fits(1620 * MB, usable, ModelKind::Asr), Fit::Yellow);
    }

    #[test]
    fn large_v3_whisper_exceeds_a_3gb_usable_budget() {
        // 3 GB weights -> ~4.2 GB footprint, over 90% (2.7 GB) of a 3 GB
        // usable budget -> won't fit.
        assert_eq!(fits(3100 * MB, 3 * GB, ModelKind::Asr), Fit::Red);
    }

    #[test]
    fn a_small_llm_is_green_on_an_8gb_usable_budget() {
        // ~1.1 GB weights -> ~2.2 GB footprint, well under 60% (4.8 GB) of
        // an 8 GB usable budget.
        assert_eq!(fits(1120 * MB, 8 * GB, ModelKind::Llm), Fit::Green);
    }

    #[test]
    fn a_3b_llm_is_tight_on_a_4gb_usable_budget() {
        // ~2.0 GB weights -> ~3.1 GB footprint on a 4 GB usable budget:
        // over the 60% (2.4 GB) green line but under the 90% (3.6 GB)
        // yellow one.
        assert_eq!(fits(2020 * MB, 4 * GB, ModelKind::Llm), Fit::Yellow);
    }

    #[test]
    fn a_7b_class_llm_is_red_on_a_4gb_usable_budget() {
        // ~4.4 GB weights (Q4_K_M 7B-class) -> ~5.4 GB footprint, way past
        // a 4 GB usable budget -> won't fit.
        assert_eq!(fits(4400 * MB, 4 * GB, ModelKind::Llm), Fit::Red);
    }

    #[test]
    fn zero_usable_budget_is_always_red() {
        assert_eq!(fits(GB, 0, ModelKind::Asr), Fit::Red);
        assert_eq!(fits(GB, 0, ModelKind::Llm), Fit::Red);
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

    #[test]
    fn specs_fit_uses_usable_ram_not_total() {
        // Lots of total RAM but a tiny usable budget -> Red, proving the
        // budget is `usable_ram`, not `total_ram`.
        let specs = HardwareSpecs {
            total_ram: 64 * GB,
            available_ram: 64 * GB,
            usable_ram: GB / 2,
        };
        assert_eq!(specs.fit(GB, ModelKind::Asr), Fit::Red);
    }
}
