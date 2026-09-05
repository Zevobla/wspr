//! The scored report: verdicts, per-criterion results, and the aggregation
//! used to print the final grouped report with totals.

/// The outcome of checking one criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// This checker actually verified the criterion holds.
    Pass,
    /// This checker actually verified the criterion does not hold.
    Fail,
    /// Requires a human, hardware (mic/speakers), multiple OSes, or a live
    /// model/API call this checker doesn't attempt — not auto-checkable
    /// today, so never scored as pass or fail.
    NeedsBench,
    /// The check exists but the tool needed to run it wasn't available in
    /// this environment (e.g. cargo-udeps not installed). Distinct from
    /// `NeedsBench`: in principle this one *is* automatable, just not here,
    /// right now.
    Skipped,
}

/// One criterion's result: which criterion, what we found, and the exact
/// evidence backing that verdict. `evidence` is mandatory and must never be
/// empty — every Pass/Fail this checker prints is required to say *why*,
/// which is this crate's own version of the anti-slop rule (AC-03) it's
/// checking the rest of the repo for: no verdict without evidence.
pub struct CheckResult {
    pub id: &'static str,
    pub verdict: Verdict,
    pub evidence: String,
}

impl CheckResult {
    pub fn pass(id: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            id,
            verdict: Verdict::Pass,
            evidence: evidence.into(),
        }
    }

    pub fn fail(id: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            id,
            verdict: Verdict::Fail,
            evidence: evidence.into(),
        }
    }

    pub fn needs_bench(id: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            id,
            verdict: Verdict::NeedsBench,
            evidence: evidence.into(),
        }
    }

    pub fn skipped(id: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            id,
            verdict: Verdict::Skipped,
            evidence: evidence.into(),
        }
    }
}
