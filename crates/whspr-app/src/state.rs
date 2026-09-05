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
    /// Live contents of the language `text_input`, kept separate from
    /// `config.language: Option<String>` since a text widget needs a plain
    /// `String` to bind to (an empty string means "no override", i.e. `None`).
    pub language_input: String,
    /// Names of the audio input devices found at boot (see `crate::devices`).
    pub input_devices: Vec<String>,
    /// The currently selected input device name, if any. Defaults to the
    /// host's default input device.
    pub selected_device: Option<String>,
}

impl State {
    /// Builds the initial state from the config loaded at boot. Device
    /// fields start empty; `crate::app::boot` fills them in separately since
    /// enumerating devices is its own concern from loading config.
    pub fn new(config: Config) -> Self {
        let language_input = config.language.clone().unwrap_or_default();
        Self {
            hub_window: None,
            config,
            language_input,
            input_devices: Vec::new(),
            selected_device: None,
        }
    }
}

/// Messages produced by the GUI and its subscriptions.
#[derive(Debug, Clone)]
pub enum Message {
    /// The Hub window finished opening; `window::open` resolves with its id.
    HubOpened(window::Id),
    /// The user picked a new ASR backend label in the Hub.
    AsrSelected(&'static str),
    /// The user picked a new refiner backend label in the Hub.
    RefineSelected(&'static str),
    /// The user edited the language override text input in the Hub.
    LanguageChanged(String),
}
