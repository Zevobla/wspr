//! Wires up the iced `Program`: `boot` opens the Hub window, `update` handles
//! messages, and `view` renders the right content for each open window.
//!
//! Built on `iced::daemon` (rather than the simpler `iced::application`)
//! from the start, since the Flow Bar overlay needs a second, independently
//! styled window and `daemon`'s `view`/`theme`/`title` all take a
//! `window::Id` so each window can render its own content.
//!
//! ## Settings persistence (known gap)
//! `whspr_config` only exposes `load`/`load_from` today -- there is no save
//! path yet, and another in-flight branch may add first-run persistence
//! around the same time this one lands. Rather than race that work by
//! bolting a `save` function onto a shared crate, edits made in the Hub
//! (backend pickers, language, device selection) are kept in the in-memory
//! `State::config`/`State` fields only for this pass; they don't survive a
//! restart. This is a deliberate, documented scope choice, not an oversight.

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
        Message::LanguageChanged(language) => {
            state.config.language = if language.is_empty() {
                None
            } else {
                Some(language.clone())
            };
            state.language_input = language;
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
    }
}

/// Only listens for keyboard events while the Hub is actively capturing a
/// hotkey preview, so normal typing (e.g. in the language text input)
/// doesn't get swallowed or misread as a capture attempt the rest of the
/// time.
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

fn subscription(state: &State) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        hotkey_capture_subscription(state),
        worker_subscription(state),
    ])
}

fn view(state: &State, window: window::Id) -> Element<'_, Message> {
    if Some(window) == state.flow_bar_window {
        crate::flow_bar::view(state)
    } else {
        crate::hub::view(state)
    }
}
