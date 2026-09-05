//! Background worker bridging the global hotkey listener, mic capture, and
//! the whspr-core `Pipeline` into the iced GUI via a `Subscription`.
//!
//! This is the actual "hold hotkey, speak, get text" loop: press the
//! (fixed, Ctrl+Space -- see `crate::hotkey_capture`) hotkey to start
//! recording, release it to stop, transcribe, refine, and inject the
//! result into whatever has focus. Uses the `testkit` Mock/Noop backends,
//! same as whspr-cli, so the whole loop works offline with no API keys.

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::Stream;

use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{HotkeyEvent, HotkeyListener, Pipeline, PipelineState, RefineContext};
use whspr_inject::{EnigoTextSink, GlobalHotkeyListener};

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

    let mut hotkey_events = listener.subscribe();
    let mut capture: Option<whspr_audio::CaptureHandle> = None;

    while let Some(event) = hotkey_events.recv().await {
        match event {
            HotkeyEvent::Pressed => {
                if capture.is_none() {
                    match whspr_audio::start_capture() {
                        Ok(handle) => capture = Some(handle),
                        Err(error) => {
                            let _ = output.send(WorkerEvent::Failed(error.to_string())).await;
                        }
                    }
                }
            }
            HotkeyEvent::Released => {
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
        }
    }
}
