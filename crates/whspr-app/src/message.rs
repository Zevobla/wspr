//! The GUI's `Message` type: every event the Hub and its subscriptions
//! produce. Split out of `crate::state` so neither file nears the AA-06
//! line cap; re-exported there as `crate::state::Message` so the many
//! existing call sites keep resolving unchanged (the same idiom the Hub's
//! `Screen`/`SettingsSection` enums use).

use iced::window;

use crate::model_menu::{AsrOption, RefineOption};
use crate::screen::{Screen, SettingsSection};

/// Messages produced by the GUI and its subscriptions.
#[derive(Debug, Clone)]
pub enum Message {
    /// The Hub window finished opening; `window::open` resolves with its id.
    HubOpened(window::Id),
    /// The primary monitor's logical size, measured right after the Hub
    /// window opens (`window::monitor_size`). Drives shrinking + re-centering
    /// the window when the default size doesn't fit a small display; `None`
    /// (e.g. headless) leaves the default in place. See
    /// `crate::hub::fit_window_size`.
    HubMonitorMeasured(Option<iced::Size>),
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
    /// The hotplug watcher saw input devices connect or disconnect (only
    /// while `[device].device_hotplug` is on -- see `crate::devices`).
    InputDevicesChanged(whspr_audio::DeviceChange),
    /// The user asked to rebind the push-to-talk hotkey by pressing a new
    /// combo. The next captured combo is persisted to `config.hotkey` and
    /// registered on the next launch (see `crate::hotkey_capture`).
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
    /// The user asked to open the full-screen longform "note desk" mode
    /// (manual entry scaffolding for now). Handled by `crate::note_desk`.
    EnterNoteDesk,
    /// The user asked to leave the note desk and return to the normal Hub
    /// (see `crate::note_desk`). Handled by `crate::note_desk`.
    BackToDictate,
    /// The note desk's `Key / All / Kept` transcript filter changed (`crate::note_desk`).
    SetTranscriptFilter(crate::note_desk::TranscriptFilter),
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
    /// The user dismissed the status banner's notice (`State::notice`).
    DismissNotice,
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
    /// `crate::app::subscriptions::tray_poll_subscription`): drains any pending tray
    /// menu clicks (`crate::tray::Handle::poll_action`) and acts on the
    /// last one. Only ever fires once `state.tray` exists.
    TrayPoll,
    /// A tick of the tray's lingering-"Done" clock (see
    /// `crate::app::subscriptions::tray_done_subscription`): once
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
    /// The user toggled "Auto-send at each pause while you hold the hotkey"
    /// in the Capture section (see `crate::worker`'s auto-send).
    AutoSendToggled(bool),
    /// The user toggled "Copy to the clipboard instead when no text field is
    /// focused" in the Capture section (`[capture].input_field_detection`).
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
    /// The user picked the media-import sign-in browser in the Privacy
    /// section: `Some(id)` to borrow that browser's cookies, `None` for none.
    CookieBrowserChanged(Option<String>),
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
    /// Windows only: the custom caption's close button was pressed. Hides the
    /// window to the tray (the app keeps running in the background); only the
    /// tray "Quit" actually exits. Same behavior as a native close request
    /// (`HubCloseRequested`).
    #[cfg(target_os = "windows")]
    CloseHubWindow,
    /// Windows only: a press began on one of the borderless window's resize
    /// hit-test zones (a thin edge or corner strip). Carries the edge/corner
    /// direction and starts an OS resize-drag, replacing the resize border
    /// the removed system frame used to provide (see
    /// `crate::hub::caption_windows`).
    #[cfg(target_os = "windows")]
    ResizeHubWindow(iced::window::Direction),
    /// The Hub window's OS close was requested (macOS traffic-light close,
    /// Alt+F4, a window-manager close). Rather than quitting, this hides the
    /// window to the tray on macOS/Windows -- the app keeps running and only
    /// the tray "Quit" exits (the installer's Done screen promises "whspr
    /// lives in your system tray"). On Linux, where there's no tray, it exits.
    HubCloseRequested,
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
    /// A keyboard event received while the link-import modal is open (from
    /// `crate::app`'s modal-scoped keyboard subscription). Used to dismiss the
    /// dialog on Esc, which its Cancel-only close otherwise ignored.
    LinkImportKey(iced::keyboard::Event),
    /// The user edited the dialog's URL input.
    LinkImportUrl(String),
    /// "Resolve" pressed: runs `whspr_import::resolve`.
    LinkImportResolve,
    /// A `resolve` finished: media metadata or an error message. The
    /// `MediaInfo` is boxed to keep the `Message` enum (and thus the `Err`
    /// variant of the handler chain) small -- see `clippy::result_large_err`.
    LinkImportResolved(Result<Box<whspr_import::MediaInfo>, String>),
    /// Captions (`true`) vs transcribe-here (`false`) selector.
    LinkImportUseCaptions(bool),
    /// Toggles whether the chapter at this index becomes a note heading.
    LinkImportToggleChapter(usize),
    /// The user edited the "clip from" `MM:SS` input.
    LinkImportClipStart(String),
    /// The user edited the "clip to" `MM:SS` input.
    LinkImportClipEnd(String),
    /// "Open note desk": runs the chosen import, then enters the note desk.
    LinkImportConfirm,
    /// "Transcribe to Dictate": downloads the resolved link's audio and runs
    /// the same ASR pipeline as "Transcribe a file", landing the result in the
    /// Dictate transcript + History (via [`Message::FileTranscribed`]) rather
    /// than the note desk. Closes the dialog. Handled in `crate::link_import`.
    LinkImportTranscribeToDictate,
    /// A `LinkImportConfirm` import finished (boxed like `LinkImportResolved`
    /// for `clippy::result_large_err`): the imported note, or an error.
    LinkImportImported(Result<Box<crate::link_import::ImportedNote>, String>),
    /// A transcribe-here import reported progress (0..=100 percent): the
    /// audio-download phase (`downloading: true`) or whisper (`false`). Drives
    /// the dialog's bar + phase label.
    LinkImportProgress { downloading: bool, percent: u8 },
    /// The link-import thumbnail finished downloading (best-effort): the JPEG
    /// bytes, or `None` if it failed or the video had no thumbnail.
    LinkImportThumbnail(Option<Vec<u8>>),
    /// The note desk's "View code" toggle: flips between the rendered note and
    /// the raw `document_typ` source in the Typst column (see
    /// `crate::note_desk` / `crate::note_export`).
    NoteDeskToggleViewCode,
    /// "Export .typ" pressed: opens a native save dialog and writes the
    /// generated Typst source to the chosen path off the UI thread.
    NoteDeskExportTyp,
    /// A `.typ` export finished: `Ok(Some(path))` on success, `Ok(None)` when
    /// the user cancelled the dialog, or `Err(message)` on a write failure.
    NoteDeskExportTypDone(Result<Option<std::path::PathBuf>, String>),
    /// "Export PDF" pressed: opens a native save dialog, writes the Typst to a
    /// temp file, then shells out to `typst compile` off the UI thread.
    NoteDeskExportPdf,
    /// A PDF export finished: `Ok(Some(path))` on success, `Ok(None)` when the
    /// user cancelled, or `Err(message)` on a compile/IO failure.
    NoteDeskExportPdfDone(Result<Option<std::path::PathBuf>, String>),
}
