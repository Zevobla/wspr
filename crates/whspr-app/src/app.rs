//! Wires up the iced `Program`: `boot` opens the Hub window, `update` handles
//! messages, and `view` renders the right content for each open window.
//!
//! Built on `iced::daemon` (rather than the simpler `iced::application`)
//! from the start, since the Flow Bar overlay needs a second, independently
//! styled window and `daemon`'s `view`/`theme`/`title` all take a
//! `window::Id` so each window can render its own content.
//!
//! ## Settings persistence (partial)
//! `whspr_config::Config::save` now exists, but this pass only wires it up
//! for the language and speaker-embedding-model pick_lists (see
//! `persist_config`, called from their `update` arms) -- those are the two
//! settings this branch's scope specifically asked to make config-driven
//! and persisted. The ASR/refiner backend pickers and device selection
//! still only live in the in-memory `State::config`/`State` fields and
//! don't survive a restart; broadening persistence to those is a
//! deliberately separate, not-yet-scoped follow-up, not an oversight.

use iced::{window, Element, Task};

use crate::config_ui;
use crate::state::{Message, State};

const HUB_TITLE: &str = "whspr";

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
    state.selected_device = crate::devices::default_input_device_name();
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
            Task::none()
        }
        Message::FlowBarOpened(id) => {
            state.flow_bar_window = Some(id);
            Task::none()
        }
        Message::AsrSelected(label) => {
            state.config.asr = config_ui::asr_from_label(label);
            Task::none()
        }
        Message::RefineSelected(label) => {
            state.config.refine = config_ui::refine_from_label(label);
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
        Message::DeviceSelected(device) => {
            state.selected_device = Some(device);
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
        Message::Worker(event) => {
            match event {
                crate::worker::WorkerEvent::StateChanged(pipeline_state) => {
                    if pipeline_state != state.pipeline_state {
                        state.pipeline_state_since = std::time::Instant::now();
                    }
                    state.pipeline_state = pipeline_state;
                }
                crate::worker::WorkerEvent::Completed {
                    text,
                    duration_secs,
                } => {
                    state.history.push(crate::history::HistoryEntry {
                        text,
                        duration_secs: Some(duration_secs),
                    });
                    state.last_error = None;
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

fn subscription(state: &State) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        hotkey_capture_subscription(state),
        worker_subscription(state),
        flow_bar_animation_subscription(state),
    ])
}

fn view(state: &State, window: window::Id) -> Element<'_, Message> {
    if Some(window) == state.flow_bar_window {
        crate::flow_bar::view(state)
    } else {
        crate::hub::view(state)
    }
}
