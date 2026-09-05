//! App configuration: backend selection and (eventually) on-disk persistence.
//! `load()` currently always returns defaults; the config team can opt in
//! `figment`/`toml`/`directories` from this crate's own Cargo.toml to add
//! real file discovery and merging without touching anything else.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AsrChoice {
    #[default]
    WhisperLocal,
    OpenAi,
    Deepgram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefineChoice {
    #[default]
    Noop,
    OpenAi,
    Anthropic,
    LlamaLocal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub asr: AsrChoice,
    #[serde(default)]
    pub refine: RefineChoice,
    #[serde(default)]
    pub language: Option<String>,
}

/// Loads the effective config. Real file discovery (platform config dir via
/// `directories`, `figment`/`toml` merging, env overrides) lands with the
/// config team; for now this always returns defaults.
pub fn load() -> Config {
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_defaults() {
        let cfg = load();
        assert_eq!(cfg.asr, AsrChoice::WhisperLocal);
        assert_eq!(cfg.refine, RefineChoice::Noop);
        assert_eq!(cfg.language, None);
    }
}
