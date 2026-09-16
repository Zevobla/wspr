//! The worker's capture state machine: opens a capture when the hotkey goes
//! down -- on the configured device (falling back to the default one, with
//! a notice, when it is gone), with the configured gain, noise suppression
//! and any idle preroll -- then finalizes or discards it when the hotkey
//! comes up, and keeps the idle preroll monitor in line with the settings.

use iced::futures::channel::mpsc;
use iced::futures::sink::SinkExt;
use tokio::sync::mpsc::UnboundedSender;
use whspr_audio::CaptureHandle;
use whspr_config::Config;
use whspr_inject::DebounceAction;

use super::capture_plan::{
    capture_options, desired_monitor, resolve_device, trim_captured, DeviceResolution,
};
use super::hotkey_decision::{capture_decision, CaptureDecision};
use super::preroll::PrerollSlot;
use super::processor::Chunk;
use super::WorkerEvent;
use crate::sound::{play, Cue};

/// A capture in progress.
struct ActiveCapture {
    handle: CaptureHandle,
    /// The focused app when the hotkey went down (see
    /// `crate::active_window`), passed to the refiner as context.
    app_name: Option<String>,
}

/// Everything the hotkey loop needs between events: the latest settings,
/// where results and finished chunks go, the idle preroll monitor, and the
/// capture in progress (if any).
pub(super) struct Session {
    config: Config,
    output: mpsc::Sender<WorkerEvent>,
    chunks: UnboundedSender<Chunk>,
    preroll: PrerollSlot,
    active: Option<ActiveCapture>,
}

impl Session {
    /// A session with no capture in progress and no preroll monitor yet --
    /// call [`Session::sync_preroll`] to open one if the settings want it.
    pub(super) fn new(
        config: Config,
        output: mpsc::Sender<WorkerEvent>,
        chunks: UnboundedSender<Chunk>,
    ) -> Self {
        Self {
            config,
            output,
            chunks,
            preroll: PrerollSlot::default(),
            active: None,
        }
    }

    /// Acts on one debounced hotkey action (see `hotkey_decision`).
    pub(super) async fn on_action(&mut self, action: DebounceAction) {
        match capture_decision(action, self.active.is_some()) {
            CaptureDecision::Start => self.start().await,
            CaptureDecision::Finalize => self.finalize().await,
            // Drop the handle without ever stopping or transcribing it: the
            // D-10 too-short-hold outcome, so an accidental tap never
            // produces an empty transcript.
            CaptureDecision::Discard => self.active = None,
            CaptureDecision::Ignore => {}
        }
    }

    /// Adopts settings the user just saved. Capture options are read fresh
    /// at every capture start, so only a change to what the idle preroll
    /// monitor depends on needs acting on right away.
    pub(super) async fn apply_config(&mut self, config: Config) {
        let reconcile = preroll_settings_changed(&self.config, &config);
        self.config = config;
        if reconcile {
            self.sync_preroll().await;
        }
    }

    /// Opens, closes or moves the idle preroll monitor to match the current
    /// settings and connected devices.
    pub(super) async fn sync_preroll(&mut self) {
        let device = self.resolve_device();
        self.reconcile_preroll(&device).await;
    }

    /// The device a capture would open right now (see
    /// `capture_plan::resolve_device`).
    fn resolve_device(&self) -> DeviceResolution {
        resolve_device(
            self.config.device.input_device.as_deref(),
            &whspr_audio::input_device_names(),
        )
    }

    async fn reconcile_preroll(&mut self, device: &DeviceResolution) {
        if let Err(notice) = self
            .preroll
            .reconcile(desired_monitor(&self.config, device))
        {
            self.emit(WorkerEvent::Notice(notice)).await;
        }
    }

    /// Opens a capture on the resolved device with the configured options
    /// and whatever preroll the idle monitor holds for that device.
    async fn start(&mut self) {
        let device = self.resolve_device();
        if let Some(missing) = &device.missing {
            self.emit(WorkerEvent::Notice(crate::devices::fallback_notice(
                missing,
            )))
            .await;
        }
        let preroll = self.preroll.snapshot_for(&device.device);
        let options = capture_options(&self.config, device.device.clone(), preroll);
        match whspr_audio::start_capture_with(options) {
            Ok(handle) => {
                let app_name = crate::active_window::app_name_for(
                    self.config.device.active_window,
                    crate::active_window::frontmost_app_name(),
                );
                self.active = Some(ActiveCapture { handle, app_name });
                play(Cue::Start, self.config.sound.enabled);
            }
            Err(error) => self.emit(WorkerEvent::Failed(error.to_string())).await,
        }
        // The device may have changed since the monitor last opened (a
        // hotplug, a fallback): move it now so the next press gets preroll.
        self.reconcile_preroll(&device).await;
    }

    /// Stops the active capture and hands it to the processor.
    async fn finalize(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        play(Cue::Stop, self.config.sound.enabled);
        self.submit(active.handle, active.app_name).await;
    }

    /// Stops `handle`, trims the clip and queues it for transcription.
    async fn submit(&mut self, handle: CaptureHandle, app_name: Option<String>) {
        let audio = match handle.stop() {
            Ok(audio) => audio,
            Err(error) => return self.emit(WorkerEvent::Failed(error.to_string())).await,
        };
        let chunk = Chunk {
            audio: trim_captured(&audio, &self.config),
            app_name,
            config: self.config.clone(),
        };
        if self.chunks.send(chunk).is_err() {
            self.emit(WorkerEvent::Failed(
                "the transcription task stopped; restart whspr to dictate again".to_string(),
            ))
            .await;
        }
    }

    async fn emit(&mut self, event: WorkerEvent) {
        let _ = self.output.send(event).await;
    }
}

/// Whether a settings change touches what the idle preroll monitor depends
/// on: mic privacy (whether it may run at all) or the chosen input device
/// (where it listens).
fn preroll_settings_changed(old: &Config, new: &Config) -> bool {
    old.privacy.mic_privacy != new.privacy.mic_privacy
        || old.device.input_device != new.device.input_device
}
