//! Application state and message types for the whspr GUI.

use iced::window;

/// Top-level state for the whspr GUI daemon (Hub window + Flow Bar window).
#[derive(Debug, Default)]
pub struct State {
    /// Window id of the Hub window, once it has finished opening.
    pub hub_window: Option<window::Id>,
}

/// Messages produced by the GUI and its subscriptions.
#[derive(Debug, Clone)]
pub enum Message {
    /// The Hub window finished opening; `window::open` resolves with its id.
    HubOpened(window::Id),
}
