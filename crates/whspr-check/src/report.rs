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

/// Prints the full grouped report to stdout: each result under its group
/// heading (looked up from the catalog), then a headline summary line.
///
/// `results` is expected to contain at most one entry per catalog id;
/// results are printed in catalog order, grouped by the criterion's group
/// code, so the report's shape tracks `criteria::CATALOG` regardless of
/// what order checks happened to run in.
pub fn print(results: &[CheckResult]) {
    use crate::criteria;

    let mut current_group: Option<&str> = None;
    for result in results {
        let meta = criteria::lookup(result.id);
        if current_group != Some(meta.group) {
            let group_name = criteria::GROUPS
                .iter()
                .find(|(code, _)| *code == meta.group)
                .map(|(_, name)| *name)
                .unwrap_or("?");
            println!("\n== {} - {} ==", meta.group, group_name);
            current_group = Some(meta.group);
        }
        let tag = match result.verdict {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::NeedsBench => "NEEDS-BENCH",
            Verdict::Skipped => "SKIPPED",
        };
        println!("[{tag:11}] {:<7} {}", result.id, meta.title);
        println!("              {}", result.evidence);
    }

    let pass = results
        .iter()
        .filter(|r| r.verdict == Verdict::Pass)
        .count();
    let fail = results
        .iter()
        .filter(|r| r.verdict == Verdict::Fail)
        .count();
    let bench = results
        .iter()
        .filter(|r| r.verdict == Verdict::NeedsBench)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.verdict == Verdict::Skipped)
        .count();
    let auto_checkable = pass + fail;
    let not_automated = criteria::TOTAL_CRITERIA.saturating_sub(results.len());

    println!("\n== SUMMARY ==");
    println!(
        "auto-checkable: {auto_checkable}, pass: {pass}, fail: {fail}; \
         needs-bench/skipped-in-this-run: {}; not yet automated by this tool: {not_automated} \
         (of {} total acceptance-matrix criteria)",
        bench + skipped,
        criteria::TOTAL_CRITERIA
    );
    if fail > 0 {
        println!("\nFAILING criteria (highest priority for the next wave):");
        for result in results.iter().filter(|r| r.verdict == Verdict::Fail) {
            println!("  - {}: {}", result.id, criteria::lookup(result.id).title);
        }
    }
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
