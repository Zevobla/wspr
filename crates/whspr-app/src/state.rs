//! Application state and message types for the whspr GUI.

use iced::window;
use whspr_config::Config;

use crate::history::HistoryEntry;

/// Which top-level screen the Hub's tab bar (`crate::hub`) is currently
/// showing. `Dictate` is the default: whspr's core action (record/dictate)
/// gets the screen a user lands on, rather than being buried among
/// settings -- see `crate::hub`'s module doc for the redesign this drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Dictate,
    Speakers,
    History,
    Settings,
}

/// Top-level state for the whspr GUI daemon (Hub window + Flow Bar window).
#[derive(Debug)]
pub struct State {
    /// Window id of the Hub window, once it has finished opening.
    pub hub_window: Option<window::Id>,
    /// Window id of the Flow Bar overlay, once it has finished opening.
    pub flow_bar_window: Option<window::Id>,
    /// The effective config as of app start (defaults overlaid with the
    /// user's config file, per `whspr_config::load`). Edits made in the Hub
    /// are saved back to the config file immediately (see
    /// `crate::app::persist_config`), so every setting -- ASR/refiner picks,
    /// language, embedding model, toggles, and the input device -- survives
    /// a restart.
    pub config: Config,
    /// Names of the audio input devices found at boot (see `crate::devices`).
    pub input_devices: Vec<String>,
    /// The currently selected input device name, if any. At boot this is
    /// restored from `config.device.input_device` when one was persisted,
    /// otherwise it defaults to the host's default input device.
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
    /// Which screen the Hub's tab bar is currently showing. Switched by
    /// `Message::TabSelected` (see `crate::hub`'s tab bar).
    pub screen: Screen,
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
    /// Status/progress text for an in-flight or just-finished file
    /// transcription (e.g. "Transcribing note.wav..." or an error), shown in
    /// the Transcribe section. `None` when nothing is happening.
    pub transcribe_status: Option<String>,
    /// The text from the most recent "Transcribe a file" run, shown on-screen
    /// in the Hub. `None` until the user transcribes a file this session.
    pub transcribed_text: Option<String>,
    /// Whether the in-app record button is currently capturing. The live
    /// `CaptureHandle` itself lives in a main-thread `thread_local` in
    /// `crate::app` (cpal's stream is `!Send`/`!Debug`, so it can't sit in
    /// this struct); this flag mirrors it for the view + level subscription.
    pub is_recording: bool,
    /// Live microphone input level (RMS, ~0.0..1.0) while `is_recording`,
    /// polled by the `MicLevelTick` subscription and shown as a meter.
    pub mic_level: f32,
    /// When `pipeline_state` most recently changed. The Flow Bar times its
    /// per-state animation (pulse/sweep phase, fade-in progress -- see
    /// `crate::flow_bar::animate`) from this instant rather than from app
    /// boot, so e.g. the "Done" fade always restarts from the moment a
    /// dictation turn actually completes.
    pub pipeline_state_since: std::time::Instant,
    /// The system tray icon (B-11), if this platform supports one -- see
    /// `crate::tray`'s module doc comment for which do. `None` until
    /// lazily created on the first `HubOpened` message (see
    /// `crate::app::update`) -- never eagerly in `boot`/a `Task`, since
    /// `tray::Handle::create` needs iced's winit event loop to already be
    /// running on the calling thread.
    pub tray: Option<crate::tray::Handle>,
}

impl State {
    /// Builds the initial state from the config loaded at boot. Device
    /// fields start empty; `crate::app::boot` fills them in separately since
    /// enumerating devices is its own concern from loading config.
    pub fn new(config: Config) -> Self {
        Self {
            hub_window: None,
            flow_bar_window: None,
            config,
            input_devices: Vec::new(),
            selected_device: None,
            hotkey_capturing: false,
            captured_hotkey: None,
            history: Vec::new(),
            theme: iced::Theme::Light,
            screen: Screen::default(),
            pipeline_state: whspr_core::PipelineState::Idle,
            last_error: None,
            speaker_db: whspr_config::SpeakerDb::default(),
            speaker_rename_drafts: std::collections::HashMap::new(),
            diarize_status: None,
            transcribe_status: None,
            transcribed_text: None,
            is_recording: false,
            mic_level: 0.0,
            pipeline_state_since: std::time::Instant::now(),
            tray: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_new_initializes_defaults() {
        let config = whspr_config::Config::default();
        let state = State::new(config);

        assert!(state.hub_window.is_none());
        assert!(state.flow_bar_window.is_none());
        assert!(state.input_devices.is_empty());
        assert!(state.selected_device.is_none());
        assert!(!state.hotkey_capturing);
        assert!(state.captured_hotkey.is_none());
        assert!(state.history.is_empty());
        assert_eq!(state.theme, iced::Theme::Light);
        assert_eq!(state.screen, Screen::Dictate);
        assert_eq!(state.pipeline_state, whspr_core::PipelineState::Idle);
        assert!(state.last_error.is_none());
        assert!(state.speaker_rename_drafts.is_empty());
        assert!(state.diarize_status.is_none());
        assert!(state.tray.is_none());
    }

    #[test]
    fn screen_default_is_dictate() {
        assert_eq!(Screen::default(), Screen::Dictate);
    }

    #[test]
    fn state_preserves_initial_state() {
        let config = whspr_config::Config::default();
        let state = State::new(config);
        // Verify that the state has correct default values after init
        assert_eq!(state.theme, iced::Theme::Light);
        assert_eq!(state.pipeline_state, whspr_core::PipelineState::Idle);
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
    /// The user picked a language override label in the Hub's `pick_list`
    /// ("auto" means no override, i.e. `config.language = None`). Persisted
    /// immediately -- see `crate::app::persist_config`.
    LanguageChanged(String),
    /// The user picked a new speaker-embedding-model label in the Hub.
    /// Persisted immediately -- see `crate::app::persist_config`.
    EmbeddingModelSelected(&'static str),
    /// The user toggled "Launch at login" in the Hub. Persisted
    /// immediately and also writes/removes the OS-level autostart entry
    /// -- see `crate::app`'s `AutostartToggled` handler.
    AutostartToggled(bool),
    /// The user toggled "Play a sound on start/stop" in the Hub. Persisted
    /// immediately -- see `crate::app`'s `SoundFeedbackToggled` handler.
    SoundFeedbackToggled(bool),
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
    /// The user clicked a Hub tab bar entry: switches which screen renders
    /// below the tab bar (see `Screen`).
    TabSelected(Screen),
    /// The user clicked "Copy" on the Dictate screen: copies the current
    /// recognized transcript to the system clipboard. A no-op if there's no
    /// transcript yet -- see `crate::hub::dictate`'s `copy_enabled`, which
    /// also disables the button in that case.
    CopyTranscript,
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
    /// The user clicked "Transcribe a file" -- opens a native file picker.
    PickFileToTranscribe,
    /// The transcribe file picker resolved (`None` if the user cancelled).
    FileToTranscribePicked(Option<std::path::PathBuf>),
    /// A background file-transcription run finished: the recognized text, or
    /// an error message. Shown in the Hub's Transcribe section (no injection).
    FileTranscribed(Result<String, String>),
    /// The user clicked the in-app Record button: starts capture if idle,
    /// stops + transcribes if already recording.
    ToggleRecording,
    /// A tick of the mic-level clock while recording: refreshes `mic_level`
    /// from the live capture handle for the meter.
    MicLevelTick,
    /// The user edited a speaker's rename `text_input`: (speaker id, new
    /// draft text).
    SpeakerRenameInputChanged(String, String),
    /// The user pressed "Save" on a speaker's rename: speaker id.
    SpeakerRenameSubmitted(String),
    /// A tick of the Flow Bar's animation clock (see
    /// `crate::app::flow_bar_animation_subscription`). Carries no data --
    /// its only job is to make iced re-invoke `view` while a Flow Bar
    /// animation is playing; the animation itself reads elapsed time from
    /// `State::pipeline_state_since` at render time.
    AnimationTick,
    /// A tick of the tray icon's event-poll clock (see
    /// `crate::app::tray_poll_subscription`): drains any pending tray
    /// menu clicks (`crate::tray::Handle::poll_action`) and acts on the
    /// last one. Only ever fires once `state.tray` exists.
    TrayPoll,
}
