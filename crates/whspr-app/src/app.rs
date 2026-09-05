//! Wires up the iced `Program`: `boot` opens the Hub window, `update` handles
//! messages, and `view` renders the right content for each open window.
//!
//! Built on `iced::daemon` (rather than the simpler `iced::application`)
//! from the start, since the Flow Bar overlay needs a second, independently
//! styled window and `daemon`'s `view`/`theme`/`title` all take a
//! `window::Id` so each window can render its own content.

use iced::widget::{container, text};
use iced::{window, Element, Task, Theme};

use crate::state::{Message, State};

const HUB_TITLE: &str = "whspr";

pub fn run() -> iced::Result {
    iced::daemon(boot, update, view)
        .title(HUB_TITLE)
        .theme(Theme::Light)
        .run()
}

fn boot() -> (State, Task<Message>) {
    let (_id, open) = window::open(window::Settings::default());
    (State::default(), open.map(Message::HubOpened))
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::HubOpened(id) => {
            state.hub_window = Some(id);
            Task::none()
        }
    }
}

fn view(_state: &State, _window: window::Id) -> Element<'_, Message> {
    container(text("whspr")).into()
}
