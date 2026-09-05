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

use iced::{window, Element, Task, Theme};

use crate::config_ui;
use crate::state::{Message, State};

const HUB_TITLE: &str = "whspr";

pub fn run() -> iced::Result {
    iced::daemon(boot, update, view)
        .title(HUB_TITLE)
        .theme(Theme::Light)
        .run()
}

fn boot() -> (State, Task<Message>) {
    let config = whspr_config::load();
    let (_id, open) = window::open(window::Settings::default());
    (State::new(config), open.map(Message::HubOpened))
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::HubOpened(id) => {
            state.hub_window = Some(id);
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
    }
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    crate::hub::view(state)
}
