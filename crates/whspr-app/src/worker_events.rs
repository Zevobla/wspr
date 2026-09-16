//! `Message::Worker` handling: what the Hub does with each event the
//! background dictation worker (`crate::worker`) reports. Split out of
//! `crate::app` to keep that file under the AA-06 line cap.

use iced::Task;

use crate::state::{Message, State};
use crate::tray_state::{begin_tray_done_linger, tray_done_active};
use crate::worker::WorkerEvent;

/// Applies one worker event to `state`, returning any follow-up task.
pub(crate) fn handle(state: &mut State, event: WorkerEvent) -> Task<Message> {
    match event {
        WorkerEvent::Ready(config_channel) => {
            state.worker_config = Some(config_channel);
        }
        WorkerEvent::StateChanged(pipeline_state) => {
            state.pipeline_state = pipeline_state;

            let showing_done = tray_done_active(state.tray_done_until, std::time::Instant::now());
            match (showing_done, pipeline_state) {
                // A lingering "Done" (started by the `Completed`
                // arm below) wins over a same-window Idle -- the
                // pipeline reports `Injecting` then immediately
                // `Idle`, so without this the Idle transition
                // would erase the "Done" glance the linger exists
                // to provide; `TrayDoneTick` reverts it once the
                // linger actually elapses instead.
                (true, whspr_core::PipelineState::Idle) => {}
                // Any other state change (a fresh dictation
                // starting) preempts a still-pending linger
                // instead of fighting it every tick.
                _ => {
                    if showing_done {
                        state.tray_done_until = None;
                    }
                    if let Some(tray) = &state.tray {
                        tray.set_state(pipeline_state);
                    }
                }
            }
        }
        WorkerEvent::Completed {
            text,
            duration_secs,
            embedding,
        } => {
            // Inject the dictated text into whatever app has focus.
            // This runs here in `update()` -- iced's MAIN thread, where
            // the winit/AppKit event loop lives -- rather than in the
            // background pipeline worker, because on macOS enigo's
            // synthetic input hard-traps when called off the main
            // thread while an NSApplication is running. A failure
            // degrades to an error line instead of crashing.
            match whspr_inject::EnigoTextSink.type_text(&text) {
                Ok(()) => state.last_error = None,
                Err(error) => {
                    state.last_error = Some(format!("Text injection failed: {error}"));
                }
            }
            // Also surface it on-screen in the Hub's transcription field.
            state.transcribed_text = Some(text.clone());
            state.transcribe_status = Some("Dictated".to_string());
            // Attribute a speaker from the worker's clip embedding and
            // persist the dictation to history (in memory *and* on
            // disk), exactly like the record-button/file path -- which
            // also fixes the pre-existing bug where live-hotkey
            // dictations were never saved to disk.
            let speaker_id = crate::speakers::attribute_speaker(state, embedding);
            crate::history::record_completed(state, text, Some(duration_secs), speaker_id);
            // The pipeline has no "just finished" state to glance at
            // (see `crate::tray`), so it's timed app-side here.
            begin_tray_done_linger(state);
        }
        WorkerEvent::Failed(error) => {
            state.last_error = Some(error);
        }
        WorkerEvent::Notice(notice) => {
            state.notice = Some(notice);
        }
        WorkerEvent::NeedsModel => {
            // Onboarding, not an error: no model installed yet. Kept
            // separate from `last_error` so the calm onboarding banner
            // shows instead of the red worker-error one.
            state.needs_model = true;
        }
    }
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_notice_lands_in_state_notice_not_last_error() {
        let mut state = State::new(whspr_config::Config::default());
        let _ = handle(&mut state, WorkerEvent::Notice("heads up".to_string()));
        assert_eq!(state.notice.as_deref(), Some("heads up"));
        assert!(state.last_error.is_none());
    }

    #[test]
    fn ready_stores_the_channel_the_worker_reads_settings_from() {
        let mut state = State::new(whspr_config::Config::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let _ = handle(&mut state, WorkerEvent::Ready(tx));

        let mut config = whspr_config::Config::default();
        config.capture.input_gain = 2.5;
        state
            .worker_config
            .as_ref()
            .expect("Ready should store the channel")
            .send(config)
            .expect("the receiver is still alive");
        assert_eq!(rx.try_recv().unwrap().capture.input_gain, 2.5);
    }
}
