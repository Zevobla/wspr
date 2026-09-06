//! Wires up the iced `Program`: `boot` opens the Hub window, `update` handles
//! messages, and `view` renders the right content for each open window.
//!
//! Built on `iced::daemon` (rather than the simpler `iced::application`)
//! from the start, since the Flow Bar overlay needs a second, independently
//! styled window and `daemon`'s `view`/`theme`/`title` all take a
//! `window::Id` so each window can render its own content.
//!
//! ## Settings persistence
//! Every Hub setting is written straight back to the config file the moment
//! it changes, via `persist_config` (called from the relevant `update`
//! arms): the ASR/refiner backend pickers, the language and
//! speaker-embedding-model pick_lists, the launch-at-login and
//! sound-feedback toggles, and the input-device picker. The chosen input
//! device is stored in `Config::device.input_device` and restored in
//! `boot` (falling back to the host default when nothing is persisted).

use iced::{window, Element, Task};

use crate::config_ui;
use crate::state::{Message, State};

const HUB_TITLE: &str = "whspr";

thread_local! {
    /// The live mic capture backing the in-app Record button. cpal's stream
    /// is `!Send`/`!Debug`, so it can't live in `State`; it's only ever
    /// created, polled, and dropped from `update()` on the main thread, so a
    /// `thread_local` is sound and keeps the `!Send` type off the State.
    static RECORDER: std::cell::RefCell<Option<whspr_audio::CaptureHandle>> =
        const { std::cell::RefCell::new(None) };
}

pub fn run() -> iced::Result {
    iced::daemon(boot, update, view)
        .title(HUB_TITLE)
        .theme(|state: &State, _window| state.theme.clone())
        .subscription(subscription)
        .run()
}

fn boot() -> (State, Task<Message>) {
    let config = whspr_config::load();
    let mut state = State::new(config);
    state.input_devices = crate::devices::list_input_device_names();
    // Restore the previously chosen input device if one was persisted,
    // otherwise fall back to the host's default input device.
    state.selected_device = state
        .config
        .device
        .input_device
        .clone()
        .or_else(crate::devices::default_input_device_name);
    state.history = crate::history::history_file_path()
        .map(|path| crate::history::read_history_file(&path))
        .unwrap_or_default();
    state.speaker_db = crate::speakers::speaker_db_path()
        .map(|path| whspr_config::SpeakerDb::load(&path))
        .unwrap_or_default();

    let (_id, open_hub) = window::open(window::Settings::default());
    let (_id, open_flow_bar) = window::open(crate::flow_bar::window_settings());

    let open = Task::batch([
        open_hub.map(Message::HubOpened),
        open_flow_bar.map(Message::FlowBarOpened),
    ]);

    (state, open)
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::HubOpened(id) => {
            state.hub_window = Some(id);
            // Lazily created here (never eagerly in `boot`/a `Task`) --
            // by the time the Hub has actually opened, iced's winit event
            // loop is unambiguously already running on this thread. See
            // `crate::tray`'s module doc comment for why that matters.
            if state.tray.is_none() {
                state.tray = crate::tray::Handle::create(state.pipeline_state);
            }
            Task::none()
        }
        Message::FlowBarOpened(id) => {
            state.flow_bar_window = Some(id);
            Task::none()
        }
        Message::AsrSelected(label) => {
            state.config.asr = config_ui::asr_from_label(label);
            persist_config(state);
            Task::none()
        }
        Message::RefineSelected(label) => {
            state.config.refine = config_ui::refine_from_label(label);
            persist_config(state);
            Task::none()
        }
        Message::LanguageChanged(label) => {
            state.config.language = config_ui::language_from_label(&label);
            persist_config(state);
            Task::none()
        }
        Message::EmbeddingModelSelected(label) => {
            state.config.speaker.embedding_model = config_ui::embedding_from_label(label);
            persist_config(state);
            Task::none()
        }
        Message::AutostartToggled(enabled) => {
            state.config.autostart.enabled = enabled;
            apply_autostart(state, enabled);
            persist_config(state);
            Task::none()
        }
        Message::SoundFeedbackToggled(enabled) => {
            state.config.sound.enabled = enabled;
            persist_config(state);
            Task::none()
        }
        Message::DeviceSelected(device) => {
            state.config.device.input_device = Some(device.clone());
            state.selected_device = Some(device);
            persist_config(state);
            Task::none()
        }
        Message::StartHotkeyCapture => {
            state.hotkey_capturing = true;
            Task::none()
        }
        Message::HotkeyCaptureKeyEvent(event) => {
            if let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event {
                state.captured_hotkey =
                    Some(crate::hotkey_capture::format_key_combo(modifiers, &key));
                state.hotkey_capturing = false;
            }
            Task::none()
        }
        Message::ThemeToggled => {
            state.theme = match state.theme {
                iced::Theme::Dark => iced::Theme::Light,
                _ => iced::Theme::Dark,
            };
            Task::none()
        }
        Message::TabSelected(screen) => {
            state.screen = screen;
            Task::none()
        }
        Message::CopyTranscript => match &state.transcribed_text {
            Some(text) if !text.trim().is_empty() => iced::clipboard::write(text.clone()),
            _ => Task::none(),
        },
        Message::Worker(event) => {
            match event {
                crate::worker::WorkerEvent::StateChanged(pipeline_state) => {
                    if pipeline_state != state.pipeline_state {
                        state.pipeline_state_since = std::time::Instant::now();
                    }
                    state.pipeline_state = pipeline_state;
                    if let Some(tray) = &state.tray {
                        tray.set_state(pipeline_state);
                    }
                }
                crate::worker::WorkerEvent::Completed {
                    text,
                    duration_secs,
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
                    state.history.push(crate::history::HistoryEntry {
                        text,
                        duration_secs: Some(duration_secs),
                    });
                }
                crate::worker::WorkerEvent::Failed(error) => {
                    state.last_error = Some(error);
                }
            }
            Task::none()
        }
        Message::PickRecordingToDiarize => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("WAV audio", &["wav"])
                    .pick_file()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            Message::RecordingPicked,
        ),
        Message::RecordingPicked(None) => Task::none(),
        Message::RecordingPicked(Some(path)) => {
            state.diarize_status = Some(format!("Diarizing {}...", path.display()));
            match crate::speakers::speaker_db_path() {
                Some(db_path) => Task::perform(
                    crate::speakers::run_diarize_scan(
                        path,
                        state.config.speaker.enabled,
                        state.config.speaker.model_dir.clone(),
                        state.config.speaker.embedding_model,
                        state.config.speaker.similarity_threshold,
                        state.speaker_db.clone(),
                        db_path,
                    ),
                    Message::DiarizeFinished,
                ),
                None => {
                    state.diarize_status =
                        Some("Could not determine the app data directory".to_string());
                    Task::none()
                }
            }
        }
        Message::DiarizeFinished(Ok((db, count))) => {
            state.speaker_db = db;
            state.diarize_status = Some(format!("Diarization complete: {count} turn(s) found"));
            Task::none()
        }
        Message::DiarizeFinished(Err(error)) => {
            state.diarize_status = Some(format!("Diarization failed: {error}"));
            Task::none()
        }
        Message::PickFileToTranscribe => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("WAV audio", &["wav"])
                    .pick_file()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            Message::FileToTranscribePicked,
        ),
        Message::FileToTranscribePicked(None) => Task::none(),
        Message::FileToTranscribePicked(Some(path)) => {
            state.transcribe_status = Some(format!("Transcribing {}...", path.display()));
            state.transcribed_text = None;
            Task::perform(
                crate::transcribe_file::run_transcribe(path, state.config.clone()),
                Message::FileTranscribed,
            )
        }
        Message::FileTranscribed(Ok(text)) => {
            state.transcribe_status = Some("Transcription complete".to_string());
            state.transcribed_text = Some(text);
            Task::none()
        }
        Message::FileTranscribed(Err(error)) => {
            state.transcribe_status = Some(format!("Transcription failed: {error}"));
            Task::none()
        }
        Message::ToggleRecording => {
            if state.is_recording {
                // Stop: take the handle, finalize to a 16k buffer, transcribe.
                state.is_recording = false;
                state.mic_level = 0.0;
                let handle = RECORDER.with(|r| r.borrow_mut().take());
                match handle.map(|h| h.stop()) {
                    Some(Ok(audio)) => {
                        state.transcribe_status = Some("Transcribing recording...".to_string());
                        state.transcribed_text = None;
                        Task::perform(
                            crate::transcribe_file::run_transcribe_audio(
                                audio,
                                state.config.clone(),
                            ),
                            Message::FileTranscribed,
                        )
                    }
                    Some(Err(error)) => {
                        state.transcribe_status = Some(format!("Recording failed: {error}"));
                        Task::none()
                    }
                    None => Task::none(),
                }
            } else {
                // Start capturing from the selected input device.
                match whspr_audio::start_capture_on_device(state.selected_device.as_deref()) {
                    Ok(handle) => {
                        RECORDER.with(|r| *r.borrow_mut() = Some(handle));
                        state.is_recording = true;
                        state.mic_level = 0.0;
                        state.transcribe_status =
                            Some("Recording... click Stop to transcribe".to_string());
                    }
                    Err(error) => {
                        state.transcribe_status =
                            Some(format!("Could not start recording: {error}"));
                    }
                }
                Task::none()
            }
        }
        Message::MicLevelTick => {
            if state.is_recording {
                state.mic_level = RECORDER.with(|r| {
                    r.borrow()
                        .as_ref()
                        .map(|h| h.current_level())
                        .unwrap_or(0.0)
                });
            }
            Task::none()
        }
        Message::SpeakerRenameInputChanged(id, draft) => {
            state.speaker_rename_drafts.insert(id, draft);
            Task::none()
        }
        Message::SpeakerRenameSubmitted(id) => {
            if let Some(draft) = state.speaker_rename_drafts.remove(&id) {
                if !draft.trim().is_empty() {
                    state.speaker_db.rename(&id, draft);
                    if let Some(path) = crate::speakers::speaker_db_path() {
                        let _ = state.speaker_db.save(&path);
                    }
                }
            }
            Task::none()
        }
        // No state to update -- see the variant's doc comment.
        Message::AnimationTick => Task::none(),
        Message::TrayPoll => match state
            .tray
            .as_ref()
            .and_then(crate::tray::Handle::poll_action)
        {
            Some(crate::tray::Action::ShowHub) => match state.hub_window {
                Some(id) => window::gain_focus(id),
                None => Task::none(),
            },
            Some(crate::tray::Action::Quit) => iced::exit(),
            None => Task::none(),
        },
    }
}

/// Saves `state.config` to the platform config directory immediately,
/// surfacing a failure via `state.last_error` (the same field the pipeline
/// worker uses) rather than silently dropping it -- a `pick_list` selection
/// that doesn't actually persist should be visible to the user, not just a
/// log line nobody's watching.
fn persist_config(state: &mut State) {
    let Some(dirs) = directories::ProjectDirs::from("", "", "whspr") else {
        state.last_error = Some("could not determine the app config directory".to_string());
        return;
    };
    if let Err(e) = state.config.save(dirs.config_dir()) {
        state.last_error = Some(format!("failed to save config: {e}"));
    }
}

/// Writes or removes the actual OS-level autostart entry to match
/// `enabled`, surfacing a failure via `state.last_error` -- same reasoning
/// as `persist_config`: a checkbox that silently didn't do anything to the
/// OS shouldn't look like it worked. `install_autostart` needs the
/// running app's own executable path, so this is GUI-only (`whspr-cli`
/// has no persistent process for "launch at login" to point at).
fn apply_autostart(state: &mut State, enabled: bool) {
    let result = if enabled {
        std::env::current_exe()
            .map_err(|e| format!("could not determine whspr's own executable path: {e}"))
            .and_then(|exe| whspr_config::install_autostart(&exe).map_err(|e| e.to_string()))
    } else {
        whspr_config::remove_autostart().map_err(|e| e.to_string())
    };

    if let Err(e) = result {
        state.last_error = Some(format!("failed to update launch-at-login: {e}"));
    }
}

/// Only listens for keyboard events while the Hub is actively capturing a
/// hotkey preview, so normal typing elsewhere in the Hub doesn't get
/// swallowed or misread as a capture attempt the rest of the time.
fn hotkey_capture_subscription(state: &State) -> iced::Subscription<Message> {
    if state.hotkey_capturing {
        iced::keyboard::listen().map(Message::HotkeyCaptureKeyEvent)
    } else {
        iced::Subscription::none()
    }
}

/// The pipeline worker runs for the whole lifetime of the app (it owns the
/// hotkey listener), independent of Hub UI state.
fn worker_subscription(_state: &State) -> iced::Subscription<Message> {
    iced::Subscription::run(crate::worker::pipeline_worker).map(Message::Worker)
}

/// Drives the Flow Bar's per-state animation (see `crate::flow_bar`):
/// ticks continuously while a state animates on a loop (Recording's pulse,
/// Transcribing/Refining's sweep), and briefly after entering `Injecting`
/// for its one-shot fade-in -- then stops, so an idle Flow Bar costs
/// nothing. ~60Hz is plenty smooth for a small overlay pill.
fn flow_bar_animation_subscription(state: &State) -> iced::Subscription<Message> {
    let animating = match state.pipeline_state {
        whspr_core::PipelineState::Recording
        | whspr_core::PipelineState::Transcribing
        | whspr_core::PipelineState::Refining => true,
        whspr_core::PipelineState::Injecting => {
            state.pipeline_state_since.elapsed() < crate::theme::motion::MEDIUM_4
        }
        whspr_core::PipelineState::Idle | whspr_core::PipelineState::Error => false,
    };

    if animating {
        iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::AnimationTick)
    } else {
        iced::Subscription::none()
    }
}

/// Polls the tray icon for pending menu clicks (see `crate::tray`'s module
/// doc comment for why this is polled rather than pushed). Only runs once
/// `state.tray` actually exists -- `None` on Linux, or if creation failed
/// -- so there's nothing to poll for the app's whole life there. 5Hz is
/// plenty responsive for a "Show Hub"/"Quit" click.
fn tray_poll_subscription(state: &State) -> iced::Subscription<Message> {
    if state.tray.is_some() {
        iced::time::every(std::time::Duration::from_millis(200)).map(|_| Message::TrayPoll)
    } else {
        iced::Subscription::none()
    }
}

fn subscription(state: &State) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        hotkey_capture_subscription(state),
        worker_subscription(state),
        flow_bar_animation_subscription(state),
        tray_poll_subscription(state),
        mic_level_subscription(state),
    ])
}

/// While the in-app Record button is capturing, ticks ~12x/sec so the view
/// refreshes `mic_level` for the live meter; idle otherwise.
fn mic_level_subscription(state: &State) -> iced::Subscription<Message> {
    if state.is_recording {
        iced::time::every(std::time::Duration::from_millis(80)).map(|_| Message::MicLevelTick)
    } else {
        iced::Subscription::none()
    }
}

fn view(state: &State, window: window::Id) -> Element<'_, Message> {
    if Some(window) == state.flow_bar_window {
        crate::flow_bar::view(state)
    } else {
        crate::hub::view(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_title_is_correct() {
        assert_eq!(HUB_TITLE, "whspr");
    }
}
