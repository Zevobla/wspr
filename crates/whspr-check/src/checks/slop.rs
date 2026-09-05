//! Slop-meter checks: stub-function census and (when the tool is
//! available) unused-dependency detection.

use crate::repo;
use crate::report::CheckResult;
use std::path::Path;

/// Given a `git grep -n` result line (`path:lineno:content`), returns just
/// the `content` part - so callers can tell a real stub body apart from a
/// doc comment that merely *mentions* `todo!()` in prose (this repo's
/// whspr-asr/whspr-audio crate docs do exactly that, describing what used
/// to be `todo!()` bodies before real implementations landed).
fn grep_line_content(line: &str) -> &str {
    line.splitn(3, ':').nth(2).unwrap_or(line)
}

fn is_comment_line(content: &str) -> bool {
    content.trim_start().starts_with("//")
}

/// AC-02: low count of empty/stub (todo!()/unimplemented!()) function
/// bodies. Restricted to `*.rs` files (README.md/CLAUDE.md both mention
/// "todo!()" in prose describing history, which isn't a stub function) and
/// further excludes occurrences inside `//` doc comments within those
/// files (whspr-asr/whspr-audio's own module docs describe stubs that
/// have since been replaced with real, non-todo!() code).
pub fn check_stub_function_count(root: &Path) -> CheckResult {
    let mut real_hits: Vec<String> = Vec::new();
    let mut comment_hits = 0usize;

    for pattern in ["todo!()", "unimplemented!()"] {
        let matches = match repo::git_grep(root, &["-F"], pattern, &["*.rs"]) {
            Ok(m) => m,
            Err(e) => {
                return CheckResult::fail("AC-02", format!("could not grep for {pattern}: {e}"))
            }
        };
        for line in matches {
            if is_comment_line(grep_line_content(&line)) {
                comment_hits += 1;
            } else {
                real_hits.push(line);
            }
        }
    }

    const MAX_STUBS: usize = 5;
    if real_hits.len() <= MAX_STUBS {
        CheckResult::pass(
            "AC-02",
            format!(
                "{} real todo!()/unimplemented!() call site(s) (threshold: <= {MAX_STUBS}); {} \
                 additional mention(s) in comments excluded",
                real_hits.len(),
                comment_hits
            ),
        )
    } else {
        CheckResult::fail(
            "AC-02",
            format!(
                "{} real todo!()/unimplemented!() call sites (threshold: <= {MAX_STUBS}): {}",
                real_hits.len(),
                real_hits.join("; ")
            ),
        )
    }
}

/// AC-07: no unused workspace dependencies, per `cargo-udeps`.
///
/// `cargo-udeps` requires a nightly toolchain and isn't installed in every
/// environment this checker might run in. When it's missing, this reports
/// `Skipped` (not `Pass` or `Fail`) - unlike `NeedsBench` criteria (which
/// need a human/hardware/multi-OS and can't be automated by this tool at
/// all), this one *is* automatable, just not on this machine right now.
pub fn check_unused_deps(root: &Path) -> CheckResult {
    let probe = repo::run(root, "cargo", &["udeps", "--version"]);
    let available = matches!(probe, Ok(ref out) if out.success);
    if !available {
        return CheckResult::skipped(
            "AC-07",
            "cargo-udeps is not installed in this environment (`cargo udeps --version` \
             failed) - install it (`cargo install cargo-udeps`) with a nightly toolchain to \
             actually run this check; not treated as pass or fail since the criterion itself \
             is automatable, just not verified here",
        );
    }

    match repo::run(root, "cargo", &["+nightly", "udeps", "--workspace"]) {
        Ok(out) if out.success => CheckResult::pass(
            "AC-07",
            "`cargo +nightly udeps --workspace` reported no unused dependencies",
        ),
        Ok(out) => CheckResult::fail(
            "AC-07",
            format!(
                "`cargo +nightly udeps --workspace` reported unused dependencies: {}",
                crate::util::tail(&out.stdout, 800)
            ),
        ),
        Err(e) => CheckResult::fail("AC-07", format!("could not run cargo udeps: {e}")),
    }
}
