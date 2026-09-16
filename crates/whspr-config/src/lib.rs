//! App configuration: backend selection and on-disk persistence.
//!
//! Config comes from exactly two sources, merged in priority order:
//! 1. Default config (compiled in, reproducible via the Nix build)
//! 2. The user-editable TOML file in the platform config dir (e.g.
//!    `~/.config/whspr/config.toml` on Linux), overlaid on top of the
//!    defaults.
//!
//! Deliberately *not* a source: environment variables. There is no "local
//! variable" override mechanism — the only way to change a setting is to
//! edit the config file. Don't add `std::env::var` reads here.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

mod autostart;
mod capture;
mod device;
mod hex;
pub mod history_codec;
mod huggingface;
mod injection;
mod language;
mod normalize;
mod privacy;
mod refine;
mod reload;
mod secrets;
mod sound;
mod speaker;
mod whisper;

pub use autostart::{install_autostart, remove_autostart, AutostartSettings};
pub use capture::CaptureSettings;
pub use device::DeviceSettings;
pub use huggingface::HuggingFaceSettings;
pub use injection::InjectionSettings;
pub use language::{effective_language, LanguageSettings};
pub use normalize::{NormalizeSettings, NumberFormat};
pub use privacy::PrivacySettings;
pub use refine::RefineSettings;
pub use reload::{api_key_for, config_reload};
pub use secrets::{history_key, Keystore, MemoryKeystore, MigrationReport, OsKeystore, SecretName};
pub use sound::SoundSettings;
pub use speaker::{SpeakerDb, SpeakerProfile, TurnEmbedding, TurnRef};
pub use whisper::WhisperConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AsrChoice {
    #[default]
    WhisperLocal,
    OpenAi,
    Deepgram,
    /// Apple's built-in on-device speech recognizer (`SFSpeechRecognizer`,
    /// macOS only). No model download and no network — the OS ships the
    /// dictation model. Selecting it on a non-macOS build surfaces a clear
    /// "macOS only" error at backend construction.
    AppleSpeech,
    /// Deterministic, offline stand-in (`whspr_core::testkit::MockAsr`) -
    /// never a real transcription backend. An explicit opt-in only (e.g.
    /// `whspr transcribe --asr mock`), so tests and CI can ask for it by
    /// name now that `WhisperLocal` is the CLI's real, no-flag default.
    Mock,
}

impl FromStr for AsrChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "whisper-local" | "whisper_local" | "whisperloca" => Ok(AsrChoice::WhisperLocal),
            "openai" | "open-ai" | "open_ai" => Ok(AsrChoice::OpenAi),
            "deepgram" => Ok(AsrChoice::Deepgram),
            "apple-speech" | "apple_speech" | "applespeech" | "apple" => Ok(AsrChoice::AppleSpeech),
            "mock" => Ok(AsrChoice::Mock),
            _ => Err(format!("unknown ASR choice: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefineChoice {
    #[default]
    Noop,
    OpenAi,
    Anthropic,
    LlamaLocal,
    /// Apple's on-device Foundation Models system LLM (macOS 26+, Apple
    /// Intelligence). No model download and no network. Only offered when the
    /// build includes the shim and the model is available on the machine;
    /// selecting it elsewhere surfaces a clear "unavailable" error.
    AppleFoundation,
}

impl FromStr for RefineChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "noop" => Ok(RefineChoice::Noop),
            "openai" | "open-ai" | "open_ai" => Ok(RefineChoice::OpenAi),
            "anthropic" => Ok(RefineChoice::Anthropic),
            "llama-local" | "llama_local" | "llamalocal" => Ok(RefineChoice::LlamaLocal),
            "apple-foundation" | "apple_foundation" | "applefoundation" | "apple" => {
                Ok(RefineChoice::AppleFoundation)
            }
            _ => Err(format!("unknown refine choice: {}", s)),
        }
    }
}

/// Which speaker-embedding model `whspr-diarize` should load, out of
/// however many the sherpa-onnx model zoo provides pinned Nix derivations
/// for. Mirrors `AsrChoice`'s pattern: a user-facing menu choice that maps
/// to a concrete filename, never a single hardcoded model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeakerEmbeddingChoice {
    /// WeSpeaker CAM++ (VoxCeleb-trained). The current default: broad
    /// English coverage and the first embedding model this feature shipped
    /// with (see `whspr-diarize`'s crate docs).
    #[default]
    CamPlusPlus,
    /// 3D-Speaker ERes2Net.
    Eres2Net,
}

impl SpeakerEmbeddingChoice {
    /// The filename this choice maps to inside `SpeakerSettings::model_dir`.
    /// `whspr-diarize`'s `SherpaDiarizer` loads exactly this file, never a
    /// hardcoded name.
    pub fn filename(&self) -> &'static str {
        match self {
            SpeakerEmbeddingChoice::CamPlusPlus => "embedding-campplus.onnx",
            SpeakerEmbeddingChoice::Eres2Net => "embedding-eres2net.onnx",
        }
    }
}

impl FromStr for SpeakerEmbeddingChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Normalize "_"/"+" to "-" first so "cam++"/"cam_plus_plus" and
        // friends collapse onto the same few match arms as the canonical
        // "cam-plus-plus" spelling.
        match s.to_lowercase().replace(['_', '+'], "-").as_str() {
            "cam-plus-plus" | "camplusplus" | "campplus" | "cam--" => {
                Ok(SpeakerEmbeddingChoice::CamPlusPlus)
            }
            "eres2net" | "eres2-net" => Ok(SpeakerEmbeddingChoice::Eres2Net),
            _ => Err(format!("unknown speaker embedding choice: {}", s)),
        }
    }
}

/// Settings for the speaker-fingerprinting (diarization) feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SpeakerSettings {
    /// Whether the diarization/speaker-fingerprinting feature is turned on
    /// at all. Lets a user who doesn't care about it skip the sherpa model
    /// download entirely without anything else breaking.
    pub enabled: bool,
    /// Directory containing the sherpa-onnx segmentation + embedding model
    /// files (see `whspr-diarize` for the expected filenames). `None` means
    /// not configured yet — diarization fails with an honest error until
    /// the user sets this.
    pub model_dir: Option<std::path::PathBuf>,
    /// Minimum cosine similarity to match a turn to an already-enrolled
    /// speaker rather than creating a new one. See `SpeakerDb::match_or_enroll`.
    pub similarity_threshold: f32,
    /// Which speaker-embedding model to use (see `SpeakerEmbeddingChoice`).
    pub embedding_model: SpeakerEmbeddingChoice,
}

impl Default for SpeakerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            model_dir: None,
            similarity_threshold: 0.7,
            embedding_model: SpeakerEmbeddingChoice::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub asr: AsrChoice,
    #[serde(default)]
    pub refine: RefineChoice,
    #[serde(default)]
    pub language: Option<String>,
    /// The user's chosen push-to-talk hotkey, as a `global-hotkey`-parseable
    /// combo label (e.g. `"Ctrl+Shift+D"`). `None` means "use the platform
    /// default" (`whspr_inject::default_hotkey_label`). The hotkey listener
    /// registers this combo at startup, so a change made in the Hub takes
    /// effect on the next launch rather than live.
    #[serde(default)]
    pub hotkey: Option<String>,
    /// API keys for cloud backends, keyed by backend id (e.g. "openai",
    /// matching `AsrBackend::id()` / `TextRefiner::id()`), read from the
    /// config file's `[api_keys]` table.
    ///
    /// Stored in plaintext here, but this is now the *legacy* fallback
    /// path (criterion P-06): the OS keystore is the primary store (see
    /// the `secrets` module — `Keystore`, `OsKeystore`,
    /// `SecretName::api_key`). Read a key through
    /// [`Config::resolve_api_key`] rather than this map directly — it
    /// checks the keystore first and only falls back here for a config
    /// that hasn't been migrated yet.
    /// [`Config::migrate_secrets_to_keystore`] moves entries out of this
    /// map into the keystore and blanks them; nothing does that
    /// automatically, so a config written before P-06 keeps using this
    /// field until something calls it. Never read from environment
    /// variables.
    #[serde(default)]
    pub api_keys: BTreeMap<String, String>,
    /// `WhisperLocal` (whisper-rs) settings, read from the config file's
    /// `[whisper]` table.
    #[serde(default)]
    pub whisper: WhisperConfig,
    #[serde(default)]
    pub speaker: SpeakerSettings,
    /// Rule-based text normalization settings, read from the config file's
    /// `[normalize]` table.
    #[serde(default)]
    pub normalize: NormalizeSettings,
    /// Language recognition and selection settings, read from the config
    /// file's `[language-settings]` table.
    #[serde(default)]
    pub language_settings: LanguageSettings,
    /// Audio device and window tracking settings, read from the config
    /// file's `[device]` table.
    #[serde(default)]
    pub device: DeviceSettings,
    /// Launch-at-login setting, read from the config file's `[autostart]`
    /// table.
    #[serde(default)]
    pub autostart: AutostartSettings,
    /// Start/stop sound-feedback setting, read from the config file's
    /// `[sound]` table.
    #[serde(default)]
    pub sound: SoundSettings,
    /// Text-injection tuning, read from the config file's `[injection]`
    /// table.
    #[serde(default)]
    pub injection: InjectionSettings,
    /// Privacy and security settings, read from the config file's
    /// `[privacy]` table.
    #[serde(default)]
    pub privacy: PrivacySettings,
    /// Capture and transcript handling settings, read from the config file's
    /// `[capture]` table.
    #[serde(default)]
    pub capture: CaptureSettings,
    /// In-app HuggingFace model-client settings (`[huggingface]` table).
    #[serde(default)]
    pub huggingface: HuggingFaceSettings,
    /// Model selection + instructions for the LLM refiner backends, read
    /// from the config file's `[refine_settings]` table. Named
    /// `refine_settings` rather than `refine` because that field name is
    /// already `RefineChoice` above -- mirrors the `language`/
    /// `language_settings` split.
    #[serde(default)]
    pub refine_settings: RefineSettings,
}

impl Config {
    /// Writes this config as TOML to `config_dir/config.toml`, creating the
    /// directory if needed. The save-side counterpart to `load_from` —
    /// unlike that function's best-effort, swallow-errors first-run write,
    /// this one surfaces failures to the caller (e.g. the GUI wants to know
    /// if a settings change didn't actually persist).
    pub fn save(&self, config_dir: &Path) -> whspr_core::Result<()> {
        std::fs::create_dir_all(config_dir).map_err(|e| {
            whspr_core::WhsprError::Config(format!("failed to create config dir: {e}"))
        })?;
        let toml_string = toml::to_string_pretty(self).map_err(|e| {
            whspr_core::WhsprError::Config(format!("failed to serialize config: {e}"))
        })?;
        std::fs::write(config_dir.join("config.toml"), toml_string)
            .map_err(|e| whspr_core::WhsprError::Config(format!("failed to write config: {e}")))
    }
}

/// Loads the effective config from the platform config directory.
/// Falls back gracefully to defaults on any error (config loading should never crash).
pub fn load() -> Config {
    if let Some(project_dirs) = directories::ProjectDirs::from("", "", "whspr") {
        let config_dir = project_dirs.config_dir();
        load_from(Some(config_dir))
    } else {
        // If we can't determine platform config dir, use defaults
        Config::default()
    }
}

/// Loads config from a specified directory. This is the testable version.
/// Falls back to defaults on any error.
pub fn load_from(config_dir: Option<&Path>) -> Config {
    // Start with defaults
    let mut config = Config::default();

    // Overlay the TOML file if it exists. This is the *only* override
    // mechanism whspr has — no environment variables, no other source.
    if let Some(dir) = config_dir {
        let config_path = dir.join("config.toml");
        if config_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config_path) {
                if let Ok(file_config) = toml::from_str::<Config>(&contents) {
                    config = file_config;
                }
            }
        } else {
            // First run: persist the defaults so there's a real, editable
            // file waiting for the user, instead of only ever living in
            // memory until someone creates one by hand.
            write_defaults(dir, &config_path, &config);
        }
    }

    config
}

/// Writes `config` as TOML to `config_path`, creating `dir` (and any
/// missing parent directories) first. Best-effort: a read-only or
/// otherwise uncreatable config directory must never crash `load_from`,
/// so failures here are swallowed and the in-memory defaults are used
/// regardless.
fn write_defaults(dir: &Path, config_path: &Path, config: &Config) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(toml_string) = toml::to_string_pretty(config) {
        let _ = std::fs::write(config_path, toml_string);
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
