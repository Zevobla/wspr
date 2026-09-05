//! Autonomy/privacy checks: does the mock/local path avoid the network,
//! and is the default backend selection local/offline rather than cloud.

use crate::repo;
use crate::report::CheckResult;
use std::path::Path;

/// Same poisoned-proxy trick as `tests_isolation`'s network-isolation
/// check, applied to the actual `whspr` binary instead of the test suite.
const POISON_ENV: &[(&str, &str)] = &[
    ("HTTP_PROXY", "http://127.0.0.1:9"),
    ("HTTPS_PROXY", "http://127.0.0.1:9"),
    ("ALL_PROXY", "http://127.0.0.1:9"),
];

/// P-01: `transcribe` runs with no network reachability for the
/// mock/local path. Runs the real binary against a poisoned proxy and
/// confirms it still produces the same transcript - if the default
/// pipeline (MockAsr + NoopRefiner) ever started making a real HTTP call,
/// this would fail fast against the unreachable proxy address instead.
pub fn check_transcribe_offline(bin: &Path, root: &Path) -> CheckResult {
    let fixture = repo::fixture_wav_path(root);
    let Some(fixture_str) = fixture.to_str() else {
        return CheckResult::fail("P-01", "fixture WAV path isn't valid UTF-8");
    };
    let output = repo::run_env(
        root,
        bin.to_str().unwrap_or("whspr"),
        &["transcribe", fixture_str],
        POISON_ENV,
    );
    match output {
        Ok(out) if out.success && out.stdout.contains("the quick brown fox") => CheckResult::pass(
            "P-01",
            "`whspr transcribe` on a real WAV fixture still succeeds with HTTP_PROXY/HTTPS_PROXY/\
             ALL_PROXY pointed at an unreachable address - the default mock/local pipeline makes \
             no network call",
        ),
        Ok(out) => CheckResult::fail(
            "P-01",
            format!(
                "transcribe under a poisoned proxy: success={}, stdout={:?}",
                out.success, out.stdout
            ),
        ),
        Err(e) => CheckResult::fail("P-01", format!("could not run whspr transcribe: {e}")),
    }
}

/// P-08: default backend selection is local/offline, not cloud.
///
/// Calls `whspr_config::Config::default()` directly (whspr-check depends
/// on whspr-config as a real workspace crate, not just by grepping its
/// source) and checks the defaults are `WhisperLocal`/`Noop` - neither of
/// which makes a network call, as opposed to `OpenAi`/`Deepgram`/
/// `Anthropic`. Caveat stated in the evidence: this checks the *default
/// configuration*, not a runtime guarantee that cloud backends can never
/// be reached - a user opting into OpenAI/Deepgram/Anthropic is expected
/// to cause network calls, that's not a privacy violation.
pub fn check_default_backends_are_local() -> CheckResult {
    let config = whspr_config::Config::default();
    let asr_is_local = matches!(config.asr, whspr_config::AsrChoice::WhisperLocal);
    let refine_is_local = matches!(config.refine, whspr_config::RefineChoice::Noop);

    if asr_is_local && refine_is_local {
        CheckResult::pass(
            "P-08",
            "Config::default() selects AsrChoice::WhisperLocal + RefineChoice::Noop, neither of \
             which is a cloud backend (caveat: this is a default-configuration check, not proof \
             cloud backends are unreachable once a user opts into one)",
        )
    } else {
        CheckResult::fail(
            "P-08",
            format!(
                "Config::default() = {{ asr: {:?}, refine: {:?} }} - not fully local by default",
                config.asr, config.refine
            ),
        )
    }
}
