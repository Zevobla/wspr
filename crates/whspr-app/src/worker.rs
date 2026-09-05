//! Background worker bridging the global hotkey listener, mic capture, and
//! the whspr-core `Pipeline` into the iced GUI via a `Subscription`.
//!
//! This is the actual "hold hotkey, speak, get text" loop: press the
//! (fixed, Ctrl+Space -- see `crate::hotkey_capture`) hotkey to start
//! recording, release it to stop, transcribe, refine, and inject the
//! result into whatever has focus. Uses the `testkit` Mock/Noop backends,
//! same as whspr-cli, so the whole loop works offline with no API keys.
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

use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{Pipeline, PipelineState, RefineContext};
use whspr_inject::{DebounceAction, DebouncedHotkeyListener, EnigoTextSink, GlobalHotkeyListener};

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
    // `Pipeline::with_state_callback` takes a plain synchronous `Fn`, called
    // from inside `pipeline.run()` at each transition. A tokio unbounded
    // sender's `send` is exactly that: synchronous, non-blocking, and
    // `Send + Sync` (unlike the iced/futures-channel `Sender` used for
    // `output`, whose `send` is async and needs `&mut self`).
    let (state_tx, mut state_rx) = tokio::sync::mpsc::unbounded_channel::<PipelineState>();

    let pipeline = Pipeline::new(Box::new(MockAsr::default()), Box::new(NoopRefiner))
        .with_sink(Box::new(EnigoTextSink))
        .with_state_callback(Box::new(move |state| {
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
                Ok(handle) => capture = Some(handle),
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
