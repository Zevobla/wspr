//! Background worker bridging the global hotkey listener, mic capture, and
//! the whspr-core `Pipeline` into the iced GUI via a `Subscription`.
//!
//! This is the actual "hold hotkey, speak, get text" loop: press the
//! (fixed, Ctrl+Space -- see `crate::hotkey_capture`) hotkey to start
//! recording, release it to stop, transcribe, refine, and inject the
//! result into whatever has focus. `build_asr_backend`/`build_refiner`
//! select real backends from `Config` (mirroring whspr-cli's own
//! `build_asr_backend`/`build_refiner` in `crates/whspr-cli/src/main.rs`),
//! so the app honors the user's ASR/refiner choice instead of being stuck
//! on the offline mock -- `--asr mock`/`RefineChoice::Noop` are still real,
//! selectable choices, just no longer the only ones that work.
//!
//! The raw press/release stream is run through `whspr_inject`'s
//! `DebouncedHotkeyListener` before it ever reaches this loop, so a
//! too-short tap is cancelled instead of producing an empty transcript
//! (D-10) and a double-press doesn't start a second recording on top of
//! the first (D-09) -- see `capture_decision` below for exactly how those
//! debounced actions map onto capture start/stop/discard.

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::Stream;

use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{api_key_for, AsrChoice, RefineChoice};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AsrBackend, Pipeline, PipelineState, RefineContext, TextRefiner};
use whspr_inject::{DebounceAction, DebouncedHotkeyListener, GlobalHotkeyListener};
use whspr_refine::{AnthropicRefiner, LlamaLocal, NormalizingRefiner, OpenAiRefiner};

/// Events the worker reports back to the iced app.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// The pipeline moved to a new state.
    StateChanged(PipelineState),
    /// A dictation turn finished successfully; `duration_secs` is the
    /// recorded audio's length, used for the Hub's wpm stat.
    Completed { text: String, duration_secs: f32 },
    /// Hotkey listener startup, mic capture, or a pipeline run failed.
    Failed(String),
}

/// Builds the worker stream. Meant to run for the lifetime of the app once
/// subscribed to -- see `crate::app::subscription`.
pub fn pipeline_worker() -> impl Stream<Item = WorkerEvent> {
    iced::stream::channel(100, run)
}

/// What the worker should do with the currently active capture (if any) in
/// response to a debounced hotkey action. Pure, so the CancelRecording-vs-
/// StopRecording distinction -- the entire reason this debounce wiring
/// exists -- is unit-testable without a real hotkey listener, microphone,
/// or pipeline run. Mirrors `whspr_inject`'s own pattern of pulling a
/// decision out as a pure fn (`map_hotkey_state`, `HotkeyDebouncer`'s
/// `is_real_hold`/`is_double_press`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureDecision {
    /// Start a new capture.
    Start,
    /// Finalize the active capture: stop it and run the pipeline.
    Finalize,
    /// Discard the active capture without running the pipeline -- the D-10
    /// outcome for a hold too short to be a real recording.
    Discard,
    /// Nothing to do.
    Ignore,
}

/// Builds an ASR backend from `config.asr`. Mirrors whspr-cli's
/// `build_asr_backend` (`crates/whspr-cli/src/main.rs`), minus the CLI's
/// test-only base-url/api-key overrides, which have no equivalent here.
/// Returns a plain `String` error (not `anyhow`, which whspr-app doesn't
/// otherwise depend on) so the caller can forward it directly into
/// `WorkerEvent::Failed`.
pub(crate) fn build_asr_backend(
    config: &whspr_config::Config,
) -> Result<Box<dyn AsrBackend>, String> {
    match config.asr {
        AsrChoice::Mock => Ok(Box::new(MockAsr::default())),
        AsrChoice::WhisperLocal => {
            let model_path = WhisperLocal::resolve_model_path(config.whisper.model_path.clone())
                .ok_or_else(|| {
                    "no whisper model configured: set [whisper].model_path in the config file \
                     or the WHISPER_MODEL_PATH environment variable, or pick a different ASR \
                     backend in Settings"
                        .to_string()
                })?;
            Ok(Box::new(WhisperLocal::new(model_path)))
        }
        AsrChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                "OpenAI API key not configured (set [api_keys].openai in config)".to_string()
            })?;
            Ok(Box::new(OpenAiAsr::new(api_key)))
        }
        AsrChoice::Deepgram => {
            let api_key = api_key_for(config, "deepgram").ok_or_else(|| {
                "Deepgram API key not configured (set [api_keys].deepgram in config)".to_string()
            })?;
            Ok(Box::new(DeepgramAsr::new(api_key)))
        }
    }
}

/// Builds a text refiner from `config.refine`, always wrapped in
/// `NormalizingRefiner` so rule-based number/date/time normalization runs
/// regardless of which backend produced the raw text. Mirrors whspr-cli's
/// `build_refiner` (`crates/whspr-cli/src/main.rs`).
pub(crate) fn build_refiner(config: &whspr_config::Config) -> Result<Box<dyn TextRefiner>, String> {
    let inner: Box<dyn TextRefiner> = match config.refine {
        RefineChoice::Noop => Box::new(NoopRefiner),
        RefineChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                "OpenAI API key not configured (set [api_keys].openai in config)".to_string()
            })?;
            Box::new(OpenAiRefiner::new(api_key, "gpt-4o-mini"))
        }
        RefineChoice::Anthropic => {
            let api_key = api_key_for(config, "anthropic").ok_or_else(|| {
                "Anthropic API key not configured (set [api_keys].anthropic in config)".to_string()
            })?;
            Box::new(AnthropicRefiner::new(api_key, "claude-3-5-sonnet-20241022"))
        }
        RefineChoice::LlamaLocal => Box::new(LlamaLocal::new("model.gguf")),
    };

    Ok(Box::new(NormalizingRefiner::new(
        inner,
        config.normalize.clone(),
    )))
}

fn capture_decision(action: DebounceAction, capture_active: bool) -> CaptureDecision {
    match action {
        DebounceAction::StartRecording => CaptureDecision::Start,
        DebounceAction::StopRecording if capture_active => CaptureDecision::Finalize,
        DebounceAction::CancelRecording if capture_active => CaptureDecision::Discard,
        // A Stop/Cancel with no active capture (a stray/duplicate event we
        // never started one for) and an explicit Ignore both mean the same
        // thing here: nothing to do.
        DebounceAction::StopRecording
        | DebounceAction::CancelRecording
        | DebounceAction::Ignore => CaptureDecision::Ignore,
    }
}

async fn run(mut output: mpsc::Sender<WorkerEvent>) {
    let config = whspr_config::load();

    // Building either backend can fail honestly (e.g. no whisper model
    // configured, or a cloud backend picked with no API key set) -- surface
    // that through the same `WorkerEvent::Failed` + park-forever path used
    // below for a missing hotkey listener, rather than panicking or quietly
    // falling back to the mock the user didn't ask for.
    let asr_backend = match build_asr_backend(&config) {
        Ok(backend) => backend,
        Err(error) => {
            let _ = output.send(WorkerEvent::Failed(error)).await;
            std::future::pending::<()>().await;
            return;
        }
    };
    let refiner = match build_refiner(&config) {
        Ok(refiner) => refiner,
        Err(error) => {
            let _ = output.send(WorkerEvent::Failed(error)).await;
            std::future::pending::<()>().await;
            return;
        }
    };

    // `Pipeline::with_state_callback` takes a plain synchronous `Fn`, called
    // from inside `pipeline.run()` at each transition. A tokio unbounded
    // sender's `send` is exactly that: synchronous, non-blocking, and
    // `Send + Sync` (unlike the iced/futures-channel `Sender` used for
    // `output`, whose `send` is async and needs `&mut self`).
    let (state_tx, mut state_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineState>();

    // No `TextSink`: dictation results are shown in the Hub's transcription
    // field (via `WorkerEvent::Completed`) rather than injected into the
    // focused app. Synthetic text injection needs macOS Accessibility
    // permission the dev binary isn't granted, and calling it without that
    // hard-traps the process, so the on-screen path is the reliable default.
    let pipeline =
        Pipeline::new(asr_backend, refiner).with_state_callback(Box::new(move |state| {
            let _ = state_tx.send(state);
        }));

    // Forward pipeline state changes to the app for as long as the worker
    // runs; ends when `pipeline` (and the `state_tx` its callback captured)
    // is dropped at the end of this function.
    let mut state_output = output.clone();
    tokio::spawn(async move {
        while let Some(state) = state_rx.recv().await {
            if state_output
                .send(WorkerEvent::StateChanged(state))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let listener = match GlobalHotkeyListener::new() {
        Ok(listener) => listener,
        Err(error) => {
            let _ = output
                .send(WorkerEvent::Failed(format!(
                    "hotkey listener unavailable: {error}"
                )))
                .await;
            // A `Subscription`'s stream is never allowed to end on its own,
            // so without a listener to drive a loop, park forever instead
            // of returning (which would end the stream) or busy-retrying.
            std::future::pending::<()>().await;
            return;
        }
    };

    let debounced = DebouncedHotkeyListener::new(listener);
    let mut actions = debounced.subscribe_actions();
    let mut capture: Option<whspr_audio::CaptureHandle> = None;

    while let Some(action) = actions.recv().await {
        match capture_decision(action, capture.is_some()) {
            CaptureDecision::Start => match whspr_audio::start_capture() {
                Ok(handle) => {
                    capture = Some(handle);
                    crate::sound::play(crate::sound::Cue::Start, config.sound.enabled);
                }
                Err(error) => {
                    let _ = output.send(WorkerEvent::Failed(error.to_string())).await;
                }
            },
            CaptureDecision::Discard => {
                // Drop the handle without ever calling `.stop()`/the
                // pipeline: the D-10 too-short-hold outcome, so an
                // accidental tap never produces an empty transcript.
                capture = None;
            }
            CaptureDecision::Finalize => {
                let Some(handle) = capture.take() else {
                    continue;
                };
                crate::sound::play(crate::sound::Cue::Stop, config.sound.enabled);

                match handle.stop() {
                    Ok(audio) => {
                        let duration_secs = audio.duration_secs();
                        match pipeline.run(audio, &RefineContext::default()).await {
                            Ok(text) => {
                                let _ = output
                                    .send(WorkerEvent::Completed {
                                        text,
                                        duration_secs,
                                    })
                                    .await;
                            }
                            Err(error) => {
                                let _ = output.send(WorkerEvent::Failed(error.to_string())).await;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = output.send(WorkerEvent::Failed(error.to_string())).await;
                    }
                }
            }
            CaptureDecision::Ignore => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use whspr_config::Config;

    #[test]
    fn build_asr_backend_mock_choice_succeeds() {
        let config = Config {
            asr: AsrChoice::Mock,
            ..Default::default()
        };

        let backend = build_asr_backend(&config).expect("mock backend should always build");
        assert_eq!(backend.id(), "mock");
    }

    #[test]
    fn build_asr_backend_whisper_local_uses_explicit_model_path() {
        let config = Config {
            asr: AsrChoice::WhisperLocal,
            whisper: whspr_config::WhisperConfig {
                model_path: Some(PathBuf::from("/explicit/model.bin")),
            },
            ..Default::default()
        };

        let backend = build_asr_backend(&config)
            .expect("an explicit model_path should be enough to build WhisperLocal");
        assert_eq!(backend.id(), "whisper-local");
    }

    #[test]
    fn build_asr_backend_openai_requires_an_api_key() {
        let config = Config {
            asr: AsrChoice::OpenAi,
            ..Default::default()
        };

        // `Box<dyn AsrBackend>` isn't `Debug`, so `expect_err` isn't
        // available -- match directly instead.
        match build_asr_backend(&config) {
            Ok(_) => panic!("no [api_keys].openai entry should fail, not build a backend"),
            Err(error) => assert!(error.contains("OpenAI API key")),
        }
    }

    #[test]
    fn build_refiner_noop_choice_is_wrapped_in_normalizing_refiner() {
        let config = Config {
            refine: RefineChoice::Noop,
            ..Default::default()
        };

        let refiner = build_refiner(&config).expect("noop refiner should always build");
        // NormalizingRefiner::id() delegates to the inner refiner's id (see
        // whspr-refine's normalize/mod.rs), so this also proves the wrapping
        // happened rather than returning the bare NoopRefiner.
        assert_eq!(refiner.id(), "noop");
    }

    #[test]
    fn build_refiner_anthropic_requires_an_api_key() {
        let config = Config {
            refine: RefineChoice::Anthropic,
            ..Default::default()
        };

        // `Box<dyn TextRefiner>` isn't `Debug`, so `expect_err` isn't
        // available -- match directly instead.
        match build_refiner(&config) {
            Ok(_) => panic!("no [api_keys].anthropic entry should fail, not build a refiner"),
            Err(error) => assert!(error.contains("Anthropic API key")),
        }
    }

    #[test]
    fn start_recording_with_no_active_capture_starts_one() {
        assert_eq!(
            capture_decision(DebounceAction::StartRecording, false),
            CaptureDecision::Start
        );
    }

    #[test]
    fn stop_recording_with_active_capture_finalizes() {
        assert_eq!(
            capture_decision(DebounceAction::StopRecording, true),
            CaptureDecision::Finalize
        );
    }

    /// The whole reason this wiring exists: a too-short hold must discard
    /// the capture, not finalize it -- no pipeline run, no empty transcript.
    #[test]
    fn cancel_recording_discards_without_finalizing() {
        let decision = capture_decision(DebounceAction::CancelRecording, true);
        assert_eq!(decision, CaptureDecision::Discard);
        assert_ne!(decision, CaptureDecision::Finalize);
    }

    /// A Stop/Cancel with nothing active (a stray/duplicate event) must
    /// never start a phantom pipeline run.
    #[test]
    fn stop_or_cancel_without_active_capture_is_ignored() {
        assert_eq!(
            capture_decision(DebounceAction::StopRecording, false),
            CaptureDecision::Ignore
        );
        assert_eq!(
            capture_decision(DebounceAction::CancelRecording, false),
            CaptureDecision::Ignore
        );
    }

    #[test]
    fn debounced_ignore_action_is_ignored() {
        assert_eq!(
            capture_decision(DebounceAction::Ignore, true),
            CaptureDecision::Ignore
        );
        assert_eq!(
            capture_decision(DebounceAction::Ignore, false),
            CaptureDecision::Ignore
        );
    }
}
