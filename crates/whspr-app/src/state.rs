//! Application state and message types for the whspr GUI.

use iced::window;
use whspr_config::Config;

use crate::history::HistoryEntry;

/// Top-level state for the whspr GUI daemon (Hub window + Flow Bar window).
#[derive(Debug)]
pub struct State {
    /// Window id of the Hub window, once it has finished opening.
    pub hub_window: Option<window::Id>,
    /// Window id of the Flow Bar overlay, once it has finished opening.
    pub flow_bar_window: Option<window::Id>,
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
    /// Whether the Hub is currently listening for the next keypress to
    /// preview as a hotkey (see `crate::hotkey_capture` for why this is a
    /// preview only, not something that gets applied).
    pub hotkey_capturing: bool,
    /// The most recently captured hotkey preview, formatted for display.
    pub captured_hotkey: Option<String>,
    /// Completed transcriptions: whatever was on disk at boot (see
    /// `crate::history::read_history_file`), plus any the pipeline
    /// completes during this session (session-only, not written back to
    /// disk -- see `crate::app`'s persistence note).
    pub history: Vec<HistoryEntry>,
    /// The active iced theme, toggled between `Theme::Light`/`Theme::Dark`
    /// by the Hub's theme button (see `crate::app`'s `.theme` wiring).
    pub theme: iced::Theme,
    /// The dictation pipeline's current state, driven by
    /// `crate::worker::pipeline_worker`'s `WorkerEvent::StateChanged` and
    /// shown by the Flow Bar overlay.
    pub pipeline_state: whspr_core::PipelineState,
    /// The most recent error reported by the pipeline worker (hotkey
    /// listener startup, mic capture, or a pipeline run), if any.
    pub last_error: Option<String>,
    /// The persisted speaker-enrollment database (see
    /// `whspr_config::SpeakerDb`): every distinct speaker discovered across
    /// past diarization scans. Loaded at boot, written back to
    /// `speakers.json` on every rename or completed scan.
    pub speaker_db: whspr_config::SpeakerDb,
    /// Live contents of the rename `text_input` for whichever speaker
    /// profile is currently being edited, keyed by `SpeakerProfile::id` so
    /// different rows' drafts don't clobber each other.
    pub speaker_rename_drafts: std::collections::HashMap<String, String>,
    /// Status/progress text for an in-flight or just-finished diarization
    /// scan (e.g. "Diarizing recording.wav..." or an error), shown in the
    /// Speakers section. `None` when nothing is happening.
    pub diarize_status: Option<String>,
}

impl State {
    /// Builds the initial state from the config loaded at boot. Device
    /// fields start empty; `crate::app::boot` fills them in separately since
    /// enumerating devices is its own concern from loading config.
    pub fn new(config: Config) -> Self {
        let language_input = config.language.clone().unwrap_or_default();
        Self {
            hub_window: None,
            flow_bar_window: None,
            config,
            language_input,
            input_devices: Vec::new(),
            selected_device: None,
            hotkey_capturing: false,
            captured_hotkey: None,
            history: Vec::new(),
            theme: iced::Theme::Light,
            pipeline_state: whspr_core::PipelineState::Idle,
            last_error: None,
            speaker_db: whspr_config::SpeakerDb::default(),
            speaker_rename_drafts: std::collections::HashMap::new(),
            diarize_status: None,
        }
    }
}

/// Messages produced by the GUI and its subscriptions.
#[derive(Debug, Clone)]
pub enum Message {
    /// The Hub window finished opening; `window::open` resolves with its id.
    HubOpened(window::Id),
    /// The Flow Bar overlay finished opening; `window::open` resolves with
    /// its id.
    FlowBarOpened(window::Id),
    /// The user picked a new ASR backend label in the Hub.
    AsrSelected(&'static str),
    /// The user picked a new refiner backend label in the Hub.
    RefineSelected(&'static str),
    /// The user edited the language override text input in the Hub.
    LanguageChanged(String),
    /// The user picked a different input device in the Hub.
    DeviceSelected(String),
    /// The user asked to preview a new hotkey by pressing it.
    StartHotkeyCapture,
    /// A keyboard event arrived while capturing; only `KeyPressed` is acted
    /// on (see `update`), but the subscription hands over every event since
    /// `Subscription` has no `filter_map` combinator to narrow it upstream.
    HotkeyCaptureKeyEvent(iced::keyboard::Event),
    /// The user toggled between light and dark theme in the Hub.
    ThemeToggled,
    /// An event from the background pipeline worker (see `crate::worker`):
    /// a pipeline state change, a completed dictation turn, or a failure.
    Worker(crate::worker::WorkerEvent),
    /// The user clicked "Diarize a recording" -- opens a native file picker.
    PickRecordingToDiarize,
    /// The file picker resolved (`None` if the user cancelled).
    RecordingPicked(Option<std::path::PathBuf>),
    /// A background diarization run finished: the updated speaker db and
    /// how many turns were found, or an error message.
    DiarizeFinished(Result<(whspr_config::SpeakerDb, usize), String>),
    /// The user edited a speaker's rename `text_input`: (speaker id, new
    /// draft text).
    SpeakerRenameInputChanged(String, String),
    /// The user pressed "Save" on a speaker's rename: speaker id.
    SpeakerRenameSubmitted(String),
}
