//! Wires up the iced `Program`: `boot` opens the Hub window, `update`
//! handles messages, and `view` renders the Hub.
//!
//! Built on `iced::daemon` (rather than the simpler `iced::application`):
//! unlike `application`, `daemon` doesn't tie the process lifetime to a
//! single window, which suits a menu-bar app whose tray icon and "Show
//! Hub" action need to outlive the Hub window. Its `view`/`theme`/`title`
//! all take a `window::Id`; `view` ignores it here since the Hub is the
//! only window.
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
use crate::hotkey_capture::CaptureOutcome;
use crate::state::{Message, State};
use crate::tray_state::{begin_tray_done_linger, set_pipeline_state, tray_done_active};

#[path = "app_subscriptions.rs"]
mod subscriptions;

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
        // Load the three static Archivo faces and make Regular the default,
        // so every Hub surface renders in the Modernist type family and each
        // weight resolves to its own crisp static face
        // (see `crate::theme::fonts`).
        .font(crate::theme::fonts::ARCHIVO_REGULAR)
        .font(crate::theme::fonts::ARCHIVO_SEMIBOLD)
        .font(crate::theme::fonts::ARCHIVO_EXTRABOLD)
        .default_font(crate::theme::fonts::DEFAULT)
        .theme(|state: &State, _window| state.theme.clone())
        .subscription(subscriptions::subscription)
        .run()
}

fn boot() -> (State, Task<Message>) {
    let config = whspr_config::load();
    let mut state = State::new(config);
    state.input_devices = whspr_audio::input_device_names();
    // Restore the previously chosen input device if one was persisted,
    // otherwise fall back to the host's default input device.
    state.selected_device = state
        .config
        .device
        .input_device
        .clone()
        .or_else(whspr_audio::default_input_device_name);
    state.history = crate::history::history_file_path()
        .map(|path| crate::history::read_history_file(&path))
        .unwrap_or_default();
    state.speaker_db = crate::speakers::speaker_db_path()
        .map(|path| whspr_config::SpeakerDb::load(&path))
        .unwrap_or_default();
    // Scan every model directory so the Models tab's ASR + refiner selectors
    // are populated with already-installed models right away (see `crate::hf`).
    state.hf_models = crate::hf::scan(&state.config);
    // Env-gated headless screenshot dev-path (see `crate::screenshot`).
    state.screenshot_path = crate::screenshot::path_from_env();
    crate::screenshot::apply_to_screen(&mut state);
    // Boots into the OS light/dark appearance (or the screenshot harness's
    // forced theme) -- never a hardcoded default. See `crate::system_theme`.
    crate::system_theme::boot(&mut state);

    let (_id, open_hub) = window::open(crate::hub::window_settings());

    (state, open_hub.map(Message::HubOpened))
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
            // Measure the primary monitor now the window exists, so a display
            // too small for the default size gets the window shrunk + re-
            // centered to fit (see `Message::HubMonitorMeasured`). No-op on
            // roomy monitors, where `Position::Centered` already placed it.
            window::monitor_size(id).map(Message::HubMonitorMeasured)
        }
        Message::HubMonitorMeasured(monitor) => match (state.hub_window, monitor) {
            (Some(id), Some(monitor)) => {
                let fit = crate::hub::fit_window_size(monitor);
                if fit.width < crate::hub::DEFAULT_WINDOW_SIZE.width
                    || fit.height < crate::hub::DEFAULT_WINDOW_SIZE.height
                {
                    // The primary monitor can't hold the full default size:
                    // shrink to fit and re-center so the window opens fully
                    // on-screen instead of overhanging an edge.
                    let origin = crate::hub::centered_origin(monitor, fit);
                    Task::batch([window::resize(id, fit), window::move_to(id, origin)])
                } else {
                    Task::none()
                }
            }
            _ => Task::none(),
        },
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
                match crate::hotkey_capture::capture_outcome(modifiers, &key) {
                    CaptureOutcome::Bound(combo) => {
                        // Persist the new combo so the listener picks it up on
                        // the next launch, and show it as the current binding.
                        state.captured_hotkey = Some(combo.clone());
                        state.config.hotkey = Some(combo);
                        state.hotkey_capturing = false;
                        persist_config(state);
                    }
                    CaptureOutcome::Cancelled => state.hotkey_capturing = false,
                    // A lone modifier or unusable key: keep listening.
                    CaptureOutcome::Incomplete => {}
                }
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
        Message::SystemThemeTick => crate::system_theme::tick(state),
        Message::TabSelected(screen) => {
            state.screen = screen;
            Task::none()
        }
        Message::CopyTranscript => match &state.transcribed_text {
            Some(text) if !text.trim().is_empty() => iced::clipboard::write(text.clone()),
            _ => Task::none(),
        },
        Message::Worker(event) => crate::worker_events::handle(state, event),
        Message::DismissNotice => {
            state.notice = None;
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
            // Honest diarization: only diarize when a real model resolves.
            // With none, reuse the needs-speaker-model prompt and say so
            // plainly, rather than running a MockDiarizer that fakes turns.
            if whspr_diarize::SherpaDiarizer::resolve_model_dir(
                state.config.speaker.model_dir.clone(),
            )
            .is_none()
            {
                state.needs_speaker_model = true;
                state.diarize_status =
                    Some(crate::speakers::DIARIZE_UNAVAILABLE_MESSAGE.to_string());
                return Task::none();
            }
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
        Message::FileTranscribed(Ok((text, duration_secs, embedding))) => {
            state.transcribe_status = Some("Transcription complete".to_string());
            state.transcribed_text = Some(text.clone());
            let speaker_id = crate::speakers::attribute_speaker(state, embedding);
            crate::history::record_completed(state, text, Some(duration_secs), speaker_id);
            // Back to Idle so the linger reverts to Idle, then show "Done".
            state.pipeline_state = whspr_core::PipelineState::Idle;
            begin_tray_done_linger(state);
            Task::none()
        }
        Message::FileTranscribed(Err(error)) => {
            state.transcribe_status = Some(format!("Transcription failed: {error}"));
            set_pipeline_state(state, whspr_core::PipelineState::Idle);
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
                        // Show the "thinking" tray icon while transcription runs.
                        set_pipeline_state(state, whspr_core::PipelineState::Transcribing);
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
                        set_pipeline_state(state, whspr_core::PipelineState::Idle);
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
                        // Turn the tray red-mic, mirroring the hotkey path.
                        set_pipeline_state(state, whspr_core::PipelineState::Recording);
                    }
                    Err(error) => {
                        state.transcribe_status =
                            Some(format!("Could not start recording: {error}"));
                        set_pipeline_state(state, whspr_core::PipelineState::Idle);
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
        Message::TrayPoll => match state
            .tray
            .as_ref()
            .and_then(crate::tray::Handle::poll_action)
        {
            // Re-show: un-hide (the close button hides to the tray -- see
            // `hide_or_exit_hub`) *and* raise/focus, so "Show Hub" works
            // whether the window is merely behind others or hidden.
            Some(crate::tray::Action::ShowHub) => match state.hub_window {
                Some(id) => Task::batch([
                    window::set_mode(id, window::Mode::Windowed),
                    window::gain_focus(id),
                ]),
                None => Task::none(),
            },
            Some(crate::tray::Action::Quit) => iced::exit(),
            None => Task::none(),
        },
        Message::HubCloseRequested => hide_or_exit_hub(state),
        Message::TrayDoneTick => {
            if !tray_done_active(state.tray_done_until, std::time::Instant::now()) {
                state.tray_done_until = None;
                if let Some(tray) = &state.tray {
                    tray.set_state(state.pipeline_state);
                }
            }
            Task::none()
        }
        Message::DragHubWindow => match state.hub_window {
            Some(id) => window::drag(id),
            None => Task::none(),
        },
        // The Windows caption controls (the borderless window has no system
        // title bar -- see `crate::hub::window_settings`). Each drives an
        // iced 0.14 window command; `close` hides to the tray (like a native
        // close request), leaving the tray "Quit" as the only exit.
        #[cfg(target_os = "windows")]
        Message::MinimizeHubWindow => match state.hub_window {
            Some(id) => window::minimize(id, true),
            None => Task::none(),
        },
        #[cfg(target_os = "windows")]
        Message::ToggleMaximizeHubWindow => match state.hub_window {
            Some(id) => window::toggle_maximize(id),
            None => Task::none(),
        },
        #[cfg(target_os = "windows")]
        Message::CloseHubWindow => hide_or_exit_hub(state),
        #[cfg(target_os = "windows")]
        Message::ResizeHubWindow(direction) => match state.hub_window {
            Some(id) => window::drag_resize(id, direction),
            None => Task::none(),
        },
        Message::TakeScreenshot => match state.hub_window {
            Some(id) if !state.screenshot_taken => {
                state.screenshot_taken = true;
                window::screenshot(id).map(Message::ScreenshotTaken)
            }
            _ => Task::none(),
        },
        Message::ScreenshotTaken(shot) => {
            if let Some(path) = state.screenshot_path.clone() {
                if let Err(e) = crate::screenshot::save(&path, &shot) {
                    eprintln!("whspr screenshot failed: {e}");
                }
            }
            iced::exit()
        }
        // Note-desk enter/leave is handled by `crate::note_desk`; anything it
        // doesn't own falls through to the Models-tab (HuggingFace) handler
        // and then the Settings handler. All three live outside this file so
        // it stays under the 600-line cap (AA-06).
        other => match crate::link_import::update(state, &other) {
            Some(task) => task,
            None => match crate::note_desk::update(state, &other) {
                Some(task) => task,
                None => match crate::hf::update(state, other) {
                    Ok(task) => task,
                    Err(other) => crate::hub::settings::update(state, other),
                },
            },
        },
    }
}

/// A close request's outcome: hide the Hub window to the tray on platforms
/// that have one (macOS, Windows), leaving the app running in the background
/// so the tray "Quit" is the only thing that exits (matching the installer's
/// "whspr lives in your system tray" promise). Where there's no tray (Linux),
/// a close exits the app, since there'd be no way to bring it back.
fn hide_or_exit_hub(state: &State) -> Task<Message> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        match state.hub_window {
            Some(id) => window::set_mode(id, window::Mode::Hidden),
            None => Task::none(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = state;
        iced::exit()
    }
}

/// Saves `state.config` to the platform config directory immediately,
/// surfacing a failure via `state.last_error` (the same field the pipeline
/// worker uses) rather than silently dropping it -- a `pick_list` selection
/// that doesn't actually persist should be visible to the user, not just a
/// log line nobody's watching. Also hands the new config to the running
/// dictation worker (`State::worker_config`), so the change applies live.
pub(crate) fn persist_config(state: &mut State) {
    if let Some(worker) = &state.worker_config {
        if worker.send(state.config.clone()).is_err() {
            state.worker_config = None;
        }
    }
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

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    crate::hub::view(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_title_is_correct() {
        assert_eq!(HUB_TITLE, "whspr");
    }
}
