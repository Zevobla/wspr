//! Application state and message types for the whspr GUI.

use iced::window;
use whspr_config::Config;

/// Top-level state for the whspr GUI daemon (Hub window + Flow Bar window).
#[derive(Debug)]
pub struct State {
    /// Window id of the Hub window, once it has finished opening.
    pub hub_window: Option<window::Id>,
    /// The effective config as of app start (defaults overlaid with the
    /// user's config file, per `whspr_config::load`). Edits made in the Hub
    /// only live in this in-memory copy for now -- see the module doc on
    /// `crate::app` for why persistence is out of scope for this pass.
    pub config: Config,
}

impl State {
    /// Builds the initial state from the config loaded at boot.
    pub fn new(config: Config) -> Self {
        Self {
            hub_window: None,
            config,
        }
    }
}

/// Messages produced by the GUI and its subscriptions.
#[derive(Debug, Clone)]
pub enum Message {
    /// The Hub window finished opening; `window::open` resolves with its id.
    HubOpened(window::Id),
}
