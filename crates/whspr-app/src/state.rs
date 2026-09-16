//! Application state and message types for the whspr GUI.

use iced::window;
use whspr_config::Config;

use crate::history::HistoryEntry;
// The Hub's navigation enums live in their own module (AA-06 line cap); they
// were part of this file, so they're re-exported here to keep the existing
// `crate::state::Screen` / `crate::state::SettingsSection` paths working.
pub use crate::screen::{Screen, SettingsSection};
// The link-import dialog's state (with its handlers in `crate::link_import`);
// re-exported so `state::LinkImport` resolves like the other Hub state types.
pub use crate::link_import::LinkImport;
// The `Message` enum lives in `crate::message` (AA-06 line cap); re-exported
// so `crate::state::Message` keeps resolving unchanged for existing call sites.
pub use crate::message::Message;

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
    /// Names of the audio input devices found at boot (see
    /// `whspr_audio::input_device_names`).
    pub input_devices: Vec<String>,
    /// The currently selected input device name, if any. At boot this is
    /// restored from `config.device.input_device` when one was persisted,
    /// otherwise it defaults to the host's default input device.
    pub selected_device: Option<String>,
    /// Whether the Hub is currently listening for the next key combo to bind
    /// as the push-to-talk hotkey (see `crate::hotkey_capture`). A bound combo
    /// is persisted to `config.hotkey` and registered on the next launch.
    pub hotkey_capturing: bool,
    /// The combo captured during this session's rebind, formatted for display.
    /// `None` until the user rebinds the hotkey; the persisted value lives in
    /// `config.hotkey`.
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
    /// Set when the worker reports `WorkerEvent::NeedsModel`: the default
    /// local Whisper ASR is selected but no model file is installed yet -- a
    /// first-run onboarding state, not a failure. Drives the calm onboarding
    /// banner (see `crate::hub`'s status banner) instead of `last_error`'s red
    /// worker-error banner.
    pub needs_model: bool,
    /// A calm, non-error status line -- e.g. "your microphone is not
    /// connected, using the default one" or "no text field focused, copied to
    /// clipboard". Shown in the status banner until the user dismisses it
    /// (`Message::DismissNotice`) or a newer notice replaces it; kept apart
    /// from `last_error` so an informational message never reads as a
    /// failure.
    pub notice: Option<String>,
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
    /// `Message::TrayDoneTick` (see `crate::app::subscriptions::tray_done_subscription`)
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
    /// (see `crate::note_desk`), whose view short-circuits the normal nav-rail
    /// + header shell wholesale; `None` is the normal Hub.
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
            needs_model: false,
            notice: None,
            speaker_db: whspr_config::SpeakerDb::default(),
            speaker_rename_drafts: std::collections::HashMap::new(),
            diarize_status: None,
            transcribe_status: None,
            transcribed_text: None,
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
        assert!(state.notice.is_none());
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
