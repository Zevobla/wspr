//! Background worker bridging the global hotkey listener, mic capture, and
//! the whspr-core `Pipeline` into the iced GUI via a `Subscription`.
//!
//! This is the actual "hold hotkey, speak, get text" loop: press the
//! configured push-to-talk hotkey (`config.hotkey`, or the platform default
//! -- see `crate::hotkey_capture`) to start
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
//! the first (D-09) -- see `hotkey_decision` for exactly how those
//! debounced actions map onto capture start/stop/discard.
//!
//! Captures honour the user's `[capture]`/`[device]`/`[privacy]` settings
//! (`capture_plan`, `session`), finished clips are transcribed by a separate
//! task (`processor`) so the hotkey loop never blocks on a pipeline run, and
//! the app pushes every saved `Config` back in (`WorkerEvent::Ready`) so a
//! settings change applies to the very next dictation.

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use iced::futures::Stream;

use whspr_core::{Pipeline, PipelineState};
use whspr_inject::{DebouncedHotkeyListener, GlobalHotkeyListener};

mod auto_send;
mod backends;
mod capture_plan;
mod delivery;
mod hotkey_decision;
mod preroll;
mod processor;
mod session;

pub(crate) use backends::{build_asr_backend, build_refiner};
pub(crate) use capture_plan::capture_options;
pub use delivery::Delivery;
pub(crate) use delivery::CLIPBOARD_NOTICE;
use session::Session;

/// Events the worker reports back to the iced app.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// The worker is listening for the hotkey. Carries the channel the app
    /// pushes each saved `Config` through (see `State::worker_config`), so
    /// settings apply to the next capture without a restart.
    Ready(tokio::sync::mpsc::UnboundedSender<whspr_config::Config>),
    /// The pipeline moved to a new state.
    StateChanged(PipelineState),
    /// A dictation turn finished successfully; `duration_secs` is the
    /// recorded audio's length, used for the Hub's wpm stat. `embedding` is
    /// the per-clip speaker fingerprint (see
    /// `crate::transcribe_file::compute_embedding`), or `None` when speaker
    /// attribution isn't possible -- fed to
    /// `crate::speakers::attribute_speaker` so live dictations attribute a
    /// speaker exactly like the record-button/file path.
    Completed {
        text: String,
        duration_secs: f32,
        embedding: Option<Vec<f32>>,
        /// Whether to type the text or, with no text field focused, copy it
        /// to the clipboard (see `delivery`).
        delivery: Delivery,
    },
    /// Hotkey listener startup, mic capture, or a pipeline run failed.
    Failed(String),
    /// Something the user should know that is not a failure -- e.g. the
    /// configured microphone is gone and capture fell back to the default
    /// device. Shown as a calm, dismissible notice (`State::notice`).
    Notice(String),
    /// A first-run onboarding state, *not* a failure: the default local
    /// Whisper ASR is selected but no model file is installed yet, so there's
    /// nothing to transcribe with until the user picks one. Surfaced as a
    /// calm "pick a model" hint (see `crate::hub`'s status banner) rather than
    /// the red worker-error banner `Failed` drives.
    NeedsModel,
}

/// Builds the worker stream. Meant to run for the lifetime of the app once
/// subscribed to -- see `crate::app::subscriptions::subscription`.
pub fn pipeline_worker() -> impl Stream<Item = WorkerEvent> {
    iced::stream::channel(100, run)
}

async fn run(mut output: mpsc::Sender<WorkerEvent>) {
    let config = whspr_config::load();

    // Building either backend can fail honestly (e.g. no whisper model
    // configured, or a cloud backend picked with no API key set) -- surface
    // that through the same `WorkerEvent::Failed` + park-forever path used
    // below for a missing hotkey listener, rather than panicking or quietly
    // falling back to the mock the user didn't ask for.
    // API keys come from the OS keystore first (P-06), then `config.toml`.
    let keystore = whspr_config::OsKeystore::new();
    let asr_backend = match build_asr_backend(&config, &keystore) {
        Ok(backend) => backend,
        Err(error) => {
            // A fresh install with the default local Whisper ASR but no model
            // file yet is a first-run onboarding state, not a failure -- send
            // the calm `NeedsModel` hint instead of the red `Failed` banner.
            // Any other build failure (bad API key, etc.) stays an error.
            let event = if backends::is_missing_whisper_model(&config) {
                WorkerEvent::NeedsModel
            } else {
                WorkerEvent::Failed(error)
            };
            let _ = output.send(event).await;
            std::future::pending::<()>().await;
            return;
        }
    };
    let refiner = match build_refiner(&config, &keystore) {
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
    // Resolves to `None` (whisper's full multilingual auto-detect across
    // every language it supports, not limited to any particular pair) by
    // default, unless the user turned auto-switch off in Settings -- see
    // `whspr_config::effective_language` for the full resolution order.
    // Without this, live dictation silently ignored the configured language
    // entirely (the file-transcribe path already applied it via
    // `Config.language` directly; this makes both paths agree).
    let language = whspr_config::effective_language(&config.language_settings, &config.language);

    let pipeline = Pipeline::new(asr_backend, refiner)
        .with_language(language)
        // J-10: translate the transcription to English, if the user turned
        // that on in Settings.
        .with_translate(config.capture.translate)
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

    // Register the user's configured push-to-talk combo (persisted by the Hub
    // as `config.hotkey`), falling back to the platform default when none is
    // set. Read fresh from `config` here, so a rebind chosen in Settings takes
    // effect on the next launch.
    let listener_result = match config.hotkey.as_deref() {
        Some(label) => GlobalHotkeyListener::from_label(label),
        None => GlobalHotkeyListener::new(),
    };
    let listener = match listener_result {
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

    let chunks = processor::spawn(pipeline, output.clone());
    let debounced = DebouncedHotkeyListener::new(listener);
    let mut actions = debounced.subscribe_actions();

    // Only now that dictation can actually happen does the app get a
    // settings channel: a worker parked on a startup failure above never
    // reads one, so it never receives one to fill up.
    let (config_tx, mut config_rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = output.send(WorkerEvent::Ready(config_tx)).await;

    let mut session = Session::new(config, output, chunks);
    session.sync_preroll().await;

    let mut auto_send_tick = tokio::time::interval(auto_send::AUTO_SEND_TICK);
    auto_send_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            action = actions.recv() => match action {
                Some(action) => session.on_action(action).await,
                None => break,
            },
            _ = auto_send_tick.tick(), if session.auto_send_active() => {
                session.on_tick().await;
            }
            Some(config) = config_rx.recv() => {
                session.apply_config(newest_config(config, &mut config_rx)).await;
            }
        }
    }
}

/// Skips ahead to the most recent of any settings already queued behind
/// `config` -- dragging a slider saves on every step, and only the last one
/// matters.
fn newest_config(
    mut config: whspr_config::Config,
    queued: &mut tokio::sync::mpsc::UnboundedReceiver<whspr_config::Config>,
) -> whspr_config::Config {
    while let Ok(newer) = queued.try_recv() {
        config = newer;
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_config_skips_to_the_last_queued_save() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for gain in [1.25, 1.5, 1.75] {
            let mut config = whspr_config::Config::default();
            config.capture.input_gain = gain;
            tx.send(config).unwrap();
        }
        let first = rx.try_recv().unwrap();

        let newest = newest_config(first, &mut rx);

        assert_eq!(newest.capture.input_gain, 1.75);
        assert!(rx.try_recv().is_err(), "the queue is drained");
    }

    #[test]
    fn newest_config_keeps_the_only_save() {
        let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut config = whspr_config::Config::default();
        config.capture.input_gain = 0.5;

        assert_eq!(newest_config(config, &mut rx).capture.input_gain, 0.5);
    }
}
