//! Application state and message types for the whspr GUI.

use iced::window;
use whspr_config::Config;

use crate::history::HistoryEntry;
use crate::model_menu::{AsrOption, RefineOption};
// The Hub's navigation enums live in their own module (AA-06 line cap); they
// were part of this file, so they're re-exported here to keep the existing
// `crate::state::Screen` / `crate::state::SettingsSection` paths working.
pub use crate::screen::{Screen, SettingsSection};
// The link-import dialog's state (with its handlers in `crate::link_import`);
// re-exported so `state::LinkImport` resolves like the other Hub state types.
pub use crate::link_import::LinkImport;

/// Top-level state for the whspr GUI daemon (the Hub window).
#[derive(Debug)]
pub struct State {
    /// Window id of the Hub window, once it has finished opening.
    pub hub_window: Option<window::Id>,
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
    /// completes during this session. The record-button/file-transcribe
    /// path also appends each one back to the on-disk JSONL file (see
    /// `crate::history::record_completed`), so those survive a restart;
    /// the live hotkey path only pushes here in-memory for now (see
    /// `Message::Worker`'s `Completed` arm in `crate::app`).
    pub history: Vec<HistoryEntry>,
    /// The active iced theme. Set from the OS appearance at boot and
    /// re-synced whenever it changes (see `crate::system_theme`); the Hub's
    /// theme button (`Message::ThemeToggled`) can override it temporarily,
    /// but only until the OS appearance next actually changes or the app
    /// restarts.
    pub theme: iced::Theme,
    /// The last OS appearance `crate::system_theme` detected, kept apart
    /// from `theme` so a poll that finds *no* change doesn't clobber a
    /// manual override back to the (unchanged) system value. See
    /// `crate::system_theme`'s module doc comment.
    pub system_theme: iced::Theme,
    /// Which screen the Hub's nav rail is currently showing. Switched by
    /// `Message::TabSelected` (see `crate::hub`'s nav rail).
    pub screen: Screen,
    /// Which Settings sub-nav section is selected (the three-column
    /// Settings layout). Switched by `Message::SettingsSectionSelected`.
    pub settings_section: SettingsSection,
    /// Live contents of the History screen's search box; filters the
    /// history table client-side. Empty means "show everything".
    pub history_search: String,
    /// The dictation pipeline's current state, driven by
    /// `crate::worker::pipeline_worker`'s `WorkerEvent::StateChanged` and
    /// reflected by the menu-bar tray icon (see `crate::tray`).
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
    /// Live contents of the Dictate screen's "Transcribe from URL" text input
    /// (a media link -- YouTube etc.). Cleared the moment a fetch is kicked
    /// off (see `crate::app`'s `TranscribeUrlSubmit` handler); the fetch +
    /// transcription then run through the shared `FileTranscribed` path.
    pub transcribe_url_input: String,
    /// Whether the in-app record button is currently capturing. The live
    /// `CaptureHandle` itself lives in a main-thread `thread_local` in
    /// `crate::app` (cpal's stream is `!Send`/`!Debug`, so it can't sit in
    /// this struct); this flag mirrors it for the view + level subscription.
    pub is_recording: bool,
    /// Live microphone input level (RMS, ~0.0..1.0) while `is_recording`,
    /// polled by the `MicLevelTick` subscription and shown as a meter.
    pub mic_level: f32,
    /// The system tray icon (B-11), if this platform supports one -- see
    /// `crate::tray`'s module doc comment for which do. `None` until
    /// lazily created on the first `HubOpened` message (see
    /// `crate::app::update`) -- never eagerly in `boot`/a `Task`, since
    /// `tray::Handle::create` needs iced's winit event loop to already be
    /// running on the calling thread.
    pub tray: Option<crate::tray::Handle>,
    /// When set, the tray icon is showing a lingering "Done" display (see
    /// `crate::tray::TrayVisual::Done`) that should revert once
    /// `std::time::Instant::now()` passes this deadline. Set by
    /// `Message::Worker`'s `Completed` arm, cleared by
    /// `Message::TrayDoneTick` (see `crate::app::tray_done_subscription`)
    /// -- `None` whenever no dictation has completed recently enough to
    /// still be lingering.
    pub tray_done_until: Option<std::time::Instant>,
    /// Live contents of the Capture section's "Refine timeout (ms)"
    /// `text_input`, decoupled from `config.capture.refine_timeout_ms`
    /// itself (a `u64`) so a keystroke that doesn't yet parse -- e.g. the
    /// field is momentarily empty while the user retypes it -- doesn't get
    /// stomped back to the last-committed value on the next render. Only a
    /// successful parse writes through to `config` (see
    /// `crate::app::update`'s `RefineTimeoutMsChanged` arm).
    pub refine_timeout_draft: String,
    /// Live contents of the Injection section's "Pre-paste delay (ms)"
    /// `text_input`. Same reasoning as `refine_timeout_draft`.
    pub pre_paste_delay_draft: String,
    /// The signed-in HuggingFace username (from the OAuth `whoami` call), if
    /// a login completed this session. `None` when signed out; a saved token
    /// alone doesn't populate this (we don't re-run `whoami` at boot).
    pub hf_username: Option<String>,
    /// Status/progress text for the Models tab (sign-in, download, "use this
    /// model" outcomes). `None` when nothing is happening.
    pub hf_status: Option<String>,
    /// Whether a HuggingFace login or model download is in flight -- disables
    /// the Models tab's action buttons so a second one can't be kicked off.
    pub hf_busy: bool,
    /// Live progress for the model download in flight (bytes so far, total, and
    /// a smoothed rate), or `None` when idle. Armed on start, folded by
    /// `Message::HfDownloadProgress`, cleared on completion; drives the Models
    /// screen's progress bar (see `crate::hf_progress`).
    pub active_download: Option<crate::hf_progress::ActiveDownload>,
    /// Live contents of the Models tab's "sign in with a token" field: a
    /// HuggingFace access token the user pastes in place of the browser OAuth
    /// flow. Held here (never logged) only until `Message::HfTokenSubmit`
    /// validates it via `whspr_hf::oauth::whoami` and hands it to the existing
    /// `HfSignedIn` persistence path; cleared the moment submit fires.
    pub hf_token_input: String,
    /// Every model file found across the effective model directories at boot
    /// / after a download or delete (see `crate::hf::scan`), split into ASR
    /// (whisper) and LLM (GGUF refiner) buckets. Populates both unified
    /// selectors and the Models tab's per-model download/delete rows.
    pub hf_models: whspr_hf::ScanResult,
    /// This machine's RAM snapshot, probed once at boot, used for the
    /// per-model "fits your machine" badge (see `whspr_hf::HardwareSpecs`).
    pub hf_specs: whspr_hf::HardwareSpecs,
    /// Live HuggingFace GGUF search state for the refiner section (query,
    /// results, the expanded repo's file list, busy/error flags). Held in one
    /// struct so `state.rs` stays under the AA-06 line cap -- see
    /// `crate::hf::LlmSearchState`.
    pub llm_search: crate::hf::LlmSearchState,
    /// When set (from the `WHSPR_SCREENSHOT` env var at boot), the Hub
    /// window is captured to this PNG path shortly after it first renders,
    /// then the app exits -- a permission-free headless UI-verification path
    /// (see `crate::screenshot`). `None` in normal runs.
    pub screenshot_path: Option<std::path::PathBuf>,
    /// Guards the one-shot screenshot so the capture fires exactly once.
    pub screenshot_taken: bool,
    /// Set when a just-finished dictation *would* have been attributed to a
    /// speaker (speaker fingerprinting is enabled) but no diarization model
    /// is installed, so no embedding could be computed (see
    /// `crate::speakers::attribute_speaker`). The History-screen UI (a later
    /// task) reads this to raise an "install a speaker model" prompt; it
    /// stays `false` whenever attribution is disabled or a model is present.
    pub needs_speaker_model: bool,
    /// When `Some`, the Hub is in the full-screen longform "note desk" mode
    /// (see `crate::note_desk`): its view (`crate::hub::note_desk`)
    /// short-circuits the normal nav-rail + header shell wholesale. `None` is
    /// the normal Hub. Entered/left manually for now via
    /// `Message::EnterNoteDesk` / `Message::BackToDictate`; the auto-morph is
    /// a later phase.
    pub note_desk: Option<crate::note_desk::NoteDeskState>,
    /// The "Add from a link" modal dialog's state (`crate::link_import`), or
    /// `None` when the dialog is closed. Opened by `Message::LinkImportOpen`.
    pub link_import: Option<LinkImport>,
}

impl State {
    /// Builds the initial state from the config loaded at boot. Device
    /// fields start empty; `crate::app::boot` fills them in separately since
    /// enumerating devices is its own concern from loading config.
    pub fn new(config: Config) -> Self {
        let refine_timeout_draft = config.capture.refine_timeout_ms.to_string();
        let pre_paste_delay_draft = config.injection.pre_paste_delay_ms.to_string();
        // Computed before the struct literal moves `config` into place: a
        // saved token means "already signed in" even before any whoami call.
        let hf_status = config
            .huggingface
            .token
            .as_ref()
            .map(|_| "Signed in with a saved token.".to_string());
        Self {
            hub_window: None,
            config,
            input_devices: Vec::new(),
            selected_device: None,
            hotkey_capturing: false,
            captured_hotkey: None,
            history: Vec::new(),
            theme: iced::Theme::Light,
            system_theme: iced::Theme::Light,
            screen: Screen::default(),
            settings_section: SettingsSection::default(),
            history_search: String::new(),
            pipeline_state: whspr_core::PipelineState::Idle,
            last_error: None,
            speaker_db: whspr_config::SpeakerDb::default(),
            speaker_rename_drafts: std::collections::HashMap::new(),
            diarize_status: None,
            transcribe_status: None,
            transcribed_text: None,
            transcribe_url_input: String::new(),
            is_recording: false,
            mic_level: 0.0,
            tray: None,
            tray_done_until: None,
            refine_timeout_draft,
            pre_paste_delay_draft,
            hf_username: None,
            hf_status,
            hf_busy: false,
            active_download: None,
            hf_token_input: String::new(),
            hf_models: whspr_hf::ScanResult::default(),
            hf_specs: whspr_hf::probe(),
            llm_search: crate::hf::LlmSearchState::default(),
            screenshot_path: None,
            screenshot_taken: false,
            needs_speaker_model: false,
            note_desk: None,
            link_import: None,
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
        assert!(state.tray_done_until.is_none());
        assert!(state.note_desk.is_none());
    }

    #[test]
    fn state_new_seeds_refine_timeout_draft_from_config() {
        let mut config = whspr_config::Config::default();
        config.capture.refine_timeout_ms = 12345;
        let state = State::new(config);

        assert_eq!(state.refine_timeout_draft, "12345");
    }

    #[test]
    fn state_new_seeds_pre_paste_delay_draft_from_config() {
        let mut config = whspr_config::Config::default();
        config.injection.pre_paste_delay_ms = 250;
        let state = State::new(config);

        assert_eq!(state.pre_paste_delay_draft, "250");
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
    /// The user toggled between light and dark theme in the Hub. A
    /// temporary override -- see `crate::system_theme`.
    ThemeToggled,
    /// A tick of the OS-appearance poll clock (see
    /// `crate::system_theme::subscription`): re-checks the system light/dark
    /// setting and re-syncs `State::theme` if it actually changed.
    SystemThemeTick,
    /// The user clicked a nav-rail entry: switches which screen renders to
    /// the right of the rail (see `Screen`).
    TabSelected(Screen),
    /// The user asked to open the full-screen longform "note desk" mode (see
    /// `crate::note_desk`). Manual entry scaffolding for now -- a later phase
    /// replaces it with an automatic morph. Handled by `crate::note_desk`.
    EnterNoteDesk,
    /// The user asked to leave the note desk and return to the normal Hub
    /// (see `crate::note_desk`). Handled by `crate::note_desk`.
    BackToDictate,
    /// The user clicked a Settings sub-nav entry: switches which section's
    /// form renders (see `SettingsSection`).
    SettingsSectionSelected(SettingsSection),
    /// The user typed in the History screen's search box: filters the
    /// history table (see `crate::hub::history`).
    HistorySearchChanged(String),
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
    /// A background file-transcription run finished: the recognized text, the
    /// recorded audio's duration, and an optional per-clip speaker embedding
    /// (see `crate::transcribe_file::TranscribeOutcome`), or an error message.
    /// Shown in the Hub's Transcribe section (no injection) and, on success,
    /// attributed to a speaker and saved to history (see
    /// `crate::speakers::attribute_speaker` / `crate::history::record_completed`).
    FileTranscribed(Result<crate::transcribe_file::TranscribeOutcome, String>),
    /// The user edited the Dictate screen's "Transcribe from URL" text input:
    /// updates `State::transcribe_url_input`.
    TranscribeUrlInput(String),
    /// The user submitted the URL (the button or the input's Enter): if the
    /// trimmed URL is non-empty, kicks off a background fetch + transcription
    /// (see `crate::transcribe_url::run_transcribe_url`) whose result is routed
    /// through `FileTranscribed`, reusing the file-transcribe display/history/
    /// attribution path verbatim.
    TranscribeUrlSubmit,
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
    /// A tick of the tray icon's event-poll clock (see
    /// `crate::app::tray_poll_subscription`): drains any pending tray
    /// menu clicks (`crate::tray::Handle::poll_action`) and acts on the
    /// last one. Only ever fires once `state.tray` exists.
    TrayPoll,
    /// A tick of the tray's lingering-"Done" clock (see
    /// `crate::app::tray_done_subscription`): once
    /// `State::tray_done_until` has passed, reverts the tray icon back to
    /// whatever `state.pipeline_state` actually is. Only ever fires while
    /// a "Done" display is pending.
    TrayDoneTick,
    /// The user toggled "Suppress background noise" in the Capture section.
    /// Persisted immediately -- see `crate::app::persist_config`.
    NoiseSuppressionToggled(bool),
    /// The user dragged the Capture section's input-gain slider. The
    /// `iced::widget::slider` already clamps to the range it's given, so
    /// this always carries an in-range value.
    InputGainChanged(f32),
    /// The user dragged the Capture section's voice-activity-threshold
    /// slider. Same clamping note as `InputGainChanged`.
    VadThresholdChanged(f32),
    /// The user toggled "Translate to English" in the Capture section.
    TranslateToggled(bool),
    /// The user toggled "Shorten the transcript" in the Capture section.
    ShortenToggled(bool),
    /// The user toggled "Auto-send when recording pauses" in the Capture
    /// section.
    AutoSendToggled(bool),
    /// The user toggled "Detect input fields before injecting" in the
    /// Capture section.
    InputFieldDetectionToggled(bool),
    /// The user edited the Capture section's "Refine timeout (ms)"
    /// `text_input`. Always updates `State::refine_timeout_draft`; only
    /// writes through to `config.capture.refine_timeout_ms` (clamped) and
    /// persists when the text parses as a `u64` -- see `crate::app::update`.
    RefineTimeoutMsChanged(String),
    /// The user edited the Injection section's "Pre-paste delay (ms)"
    /// `text_input`. Same draft-then-parse handling as
    /// `RefineTimeoutMsChanged`.
    PrePasteDelayMsChanged(String),
    /// The user toggled "Release the microphone when not recording" in the
    /// Privacy section.
    MicPrivacyToggled(bool),
    /// The user toggled "Encrypt history at rest" in the Privacy section.
    HistoryEncryptionToggled(bool),
    /// The user toggled "Rescan devices when one is plugged/unplugged" in
    /// the Devices section.
    DeviceHotplugToggled(bool),
    /// The user toggled "Track the focused app for per-app stats" in the
    /// Devices section.
    ActiveWindowToggled(bool),
    /// The user toggled "Allow Bluetooth microphones" in the Devices
    /// section.
    BluetoothSourceToggled(bool),
    /// The user toggled "Allow virtual/software audio sources" in the
    /// Devices section.
    VirtualSourceToggled(bool),
    /// The user toggled "Keep the tray icon static" in the Devices section.
    TrayStaticToggled(bool),
    /// The user toggled "Normalize spoken numbers to digits" in the
    /// Normalize section.
    NormalizeNumbersToggled(bool),
    /// The user toggled "Normalize dates to YYYY-MM-DD" in the Normalize
    /// section.
    NormalizeDatesToggled(bool),
    /// The user toggled "Normalize times to 24-hour HH:MM" in the
    /// Normalize section.
    NormalizeTimesToggled(bool),
    /// The user picked "digits" or "words" in the Normalize section's
    /// number-rendering `pick_list`.
    NumberFormatSelected(&'static str),
    /// The user toggled "Insert paragraph breaks on long pauses" in the
    /// Normalize section.
    ParagraphBreakToggled(bool),
    /// The user toggled "Auto-punctuate" in the Normalize section.
    PunctuationToggleToggled(bool),
    /// The user edited one of the API keys section's `text_input` fields:
    /// (backend id, e.g. "openai"/"anthropic"/"deepgram", new value).
    /// Written straight into `config.api_keys` -- see `crate::app::update`.
    ApiKeyChanged(&'static str, String),
    /// The user clicked "Sign in with HuggingFace" on the Models tab: starts
    /// the browser OAuth flow (see `crate::hf::run_login`).
    HfSignIn,
    /// The OAuth login finished: `(username, token)` on success, or an error
    /// message. On success the token is saved to config and installed models
    /// are rescanned.
    HfSignedIn(Result<(String, String), String>),
    /// The user edited the Models tab's "sign in with a token" field: updates
    /// `State::hf_token_input` (the pasted HuggingFace access token). Never
    /// logged -- treated as a credential.
    HfTokenInput(String),
    /// The user submitted the pasted token: validates it via
    /// `whspr_hf::oauth::whoami` and, on success, routes into the existing
    /// `HfSignedIn(Ok((username, token)))` path so the token is persisted the
    /// same way the OAuth flow persists it.
    HfTokenSubmit,
    /// The user clicked "Sign out": clears the saved token from config.
    HfSignOut,
    /// The user clicked "Download" for the curated whisper model with this id
    /// (see `whspr_hf::WhisperModel::id`): starts the background download.
    HfDownloadModel(&'static str),
    /// A byte-count update for the download in flight, bridged from the
    /// `whspr_hf` progress channel (see `crate::hf_progress`); folds into
    /// `State::active_download`. `total` is 0 until `Content-Length` is known.
    HfDownloadProgress { downloaded: u64, total: u64 },
    /// A whisper model download finished: the flat on-disk path on success,
    /// or an error message. On success the model dirs are rescanned.
    HfModelDownloaded(Result<std::path::PathBuf, String>),
    /// The user clicked "Download" for the curated GGUF refiner LLM with this
    /// id (see `whspr_hf::LlmModel::id`): starts the background download.
    HfDownloadLlm(&'static str),
    /// A refiner LLM download finished: the flat on-disk path on success, or
    /// an error message. On success the model dirs are rescanned.
    HfLlmDownloaded(Result<std::path::PathBuf, String>),
    /// The user picked an entry in the unified ASR selector (a local whisper
    /// file or a cloud backend). Writes the choice into config and persists.
    HfAsrSelected(AsrOption),
    /// The user picked an entry in the unified refiner selector (None, a
    /// cloud refiner, or a local GGUF LLM). Writes it into config and persists.
    HfRefineSelected(RefineOption),
    /// The user clicked "Delete" on a downloaded/local model file: removes the
    /// file, then rescans.
    HfDeleteModel(std::path::PathBuf),
    /// A model delete finished: the deleted path on success, or an error
    /// message. On success the model dirs are rescanned.
    HfModelDeleted(Result<std::path::PathBuf, String>),
    /// The user typed in the refiner section's HuggingFace GGUF search box.
    LlmSearchInput(String),
    /// The user submitted the GGUF search (Enter or the Search button); an
    /// empty/whitespace query is ignored (see `crate::hf`).
    LlmSearchSubmit,
    /// A GGUF repo search finished: the repo hits, or an error message.
    LlmSearchResults(Result<Vec<whspr_hf::GgufRepoHit>, String>),
    /// The user expanded a search-result repo to list its `.gguf` files
    /// (carries the repo id); clicking the open one again collapses it.
    LlmSearchSelectRepo(String),
    /// A repo's GGUF file listing finished: the files, or an error message.
    LlmSearchFiles(Result<Vec<whspr_hf::GgufFile>, String>),
    /// The user clicked Download on a searched GGUF file: `(repo, repo-relative
    /// filename)`. Routes into the existing LLM download -> rescan path so the
    /// model then appears in the refiner selector.
    LlmSearchDownload(String, String),
    /// The user clicked "Add directory": opens a native folder picker.
    HfAddModelDir,
    /// The folder picker resolved (`None` if the user cancelled). A new dir is
    /// added to `config.huggingface.model_dirs` and the models rescanned.
    HfModelDirPicked(Option<std::path::PathBuf>),
    /// The user clicked "Remove" on a model directory: drops it from
    /// `config.huggingface.model_dirs` and rescans.
    HfRemoveModelDir(std::path::PathBuf),
    /// A press began anywhere on the Hub's custom top chrome (brand row +
    /// screen header): starts an OS window-drag so the whole header acts as
    /// the title bar (the window has no system title bar -- see
    /// `crate::hub::window_settings`).
    DragHubWindow,
    /// Windows only: the custom caption's minimize button was pressed. The
    /// OS min/maximize/close buttons don't exist on the borderless Windows
    /// window (`decorations:false`), so the app draws its own and drives them
    /// through iced's window commands (see `crate::hub::caption_windows` and
    /// the handlers in `crate::app`). macOS/Linux keep the system title bar's
    /// controls, so these variants are cfg-gated off there.
    #[cfg(target_os = "windows")]
    MinimizeHubWindow,
    /// Windows only: the custom caption's maximize/restore button was pressed.
    #[cfg(target_os = "windows")]
    ToggleMaximizeHubWindow,
    /// Windows only: the custom caption's close button was pressed. Routes
    /// through the app's clean-exit path (`iced::exit`), the same one the
    /// tray "Quit" action uses.
    #[cfg(target_os = "windows")]
    CloseHubWindow,
    /// Windows only: a press began on one of the borderless window's resize
    /// hit-test zones (a thin edge or corner strip). Carries the edge/corner
    /// direction and starts an OS resize-drag, replacing the resize border
    /// the removed system frame used to provide (see
    /// `crate::hub::caption_windows`).
    #[cfg(target_os = "windows")]
    ResizeHubWindow(iced::window::Direction),
    /// Fired shortly after the Hub first renders when `WHSPR_SCREENSHOT` is
    /// set: triggers the one-shot window capture (see `crate::screenshot`).
    TakeScreenshot,
    /// The Hub window screenshot resolved: encode it to the requested PNG
    /// path and exit.
    ScreenshotTaken(iced::window::Screenshot),
    /// Opens the link-import modal (see `crate::link_import`).
    LinkImportOpen,
    /// Dismisses the link-import dialog.
    LinkImportCancel,
    /// The user edited the dialog's URL input.
    LinkImportUrl(String),
    /// "Resolve" pressed: runs `whspr_import::resolve`.
    LinkImportResolve,
    /// A `resolve` finished: media metadata or an error message.
    LinkImportResolved(Result<whspr_import::MediaInfo, String>),
    /// Captions (`true`) vs transcribe-here (`false`) selector.
    LinkImportUseCaptions(bool),
    /// Toggles whether the chapter at this index becomes a note heading.
    LinkImportToggleChapter(usize),
    /// The user edited the "clip from" `MM:SS` input.
    LinkImportClipStart(String),
    /// The user edited the "clip to" `MM:SS` input.
    LinkImportClipEnd(String),
    /// Borrow sign-in cookies from this browser (e.g. `"safari"`).
    LinkImportBorrowCookies(String),
    /// "Open note desk" -- stubbed for F3; closes the dialog + sets a status.
    LinkImportConfirm,
}
