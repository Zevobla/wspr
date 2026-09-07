//! The two unified model selectors' view models. There is ONE ASR selector
//! and ONE refiner selector; each lists local (on-disk) *and* cloud entries
//! in a single `pick_list`, with no local/online toggle.
//!
//! [`AsrOption`]/[`RefineOption`] are the pure mapping between what a
//! `pick_list` shows and the `Config` fields the pipeline reads
//! (`build_asr_backend`/`build_refiner` in `crate::worker`). Building the
//! option list, deriving the currently-selected entry, and writing a
//! selection back into `Config` all live here (not in the view) so they stay
//! unit-testable. A selection is applied by the Models tab's `update` handler
//! (`crate::hf`), which then persists the config.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use whspr_config::{AsrChoice, Config, RefineChoice};
use whspr_hf::ScanResult;

/// A friendly display name for a local model file: the curated registry label
/// if the filename is a known whisper or GGUF model, else the bare filename.
fn local_label(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if let Some(model) = whspr_hf::model_by_filename(name) {
        model.label.to_string()
    } else if let Some(model) = whspr_hf::llm_model_by_filename(name) {
        model.label.to_string()
    } else {
        name.to_string()
    }
}

/// One entry in the unified ASR selector: a whisper model file on disk, or a
/// cloud ASR backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrOption {
    /// A whisper GGML file on disk. Selecting it sets `asr = WhisperLocal`
    /// and points `whisper.model_path` at this file.
    Local(PathBuf),
    /// A cloud ASR backend (`AsrChoice::OpenAi` or `AsrChoice::Deepgram`).
    Cloud(AsrChoice),
}

impl fmt::Display for AsrOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsrOption::Local(path) => write!(f, "{} · local", local_label(path)),
            AsrOption::Cloud(AsrChoice::OpenAi) => f.write_str("OpenAI (cloud)"),
            AsrOption::Cloud(AsrChoice::Deepgram) => f.write_str("Deepgram (cloud)"),
            // WhisperLocal/Mock are never wrapped in `Cloud`; total match only.
            AsrOption::Cloud(_) => f.write_str("Local whisper model"),
        }
    }
}

/// The unified ASR selector's entries: the cloud backends first (always shown
/// so a user can pick one even before setting its API key), then every
/// whisper file discovered on disk. If the currently-configured local model
/// isn't in the scan (a path outside the scanned dirs), it's appended so the
/// pick_list can still show it as the selection.
pub fn asr_options(models: &ScanResult, config: &Config) -> Vec<AsrOption> {
    let mut options = vec![
        AsrOption::Cloud(AsrChoice::OpenAi),
        AsrOption::Cloud(AsrChoice::Deepgram),
    ];
    options.extend(models.asr.iter().map(|m| AsrOption::Local(m.path.clone())));
    if let Some(selected) = selected_asr(config) {
        if !options.contains(&selected) {
            options.push(selected);
        }
    }
    options
}

/// The entry the ASR `pick_list` should show as selected for the current
/// config, or `None` (e.g. the test-only `Mock` backend, never in the list,
/// or `WhisperLocal` with no model path set yet).
pub fn selected_asr(config: &Config) -> Option<AsrOption> {
    match config.asr {
        AsrChoice::WhisperLocal => config.whisper.model_path.clone().map(AsrOption::Local),
        AsrChoice::OpenAi => Some(AsrOption::Cloud(AsrChoice::OpenAi)),
        AsrChoice::Deepgram => Some(AsrOption::Cloud(AsrChoice::Deepgram)),
        AsrChoice::Mock => None,
    }
}

/// Writes an ASR selection into `config`: a local file switches to
/// `WhisperLocal` and sets the model path; a cloud entry switches to that
/// backend. The caller persists afterward.
pub fn apply_asr(config: &mut Config, option: &AsrOption) {
    match option {
        AsrOption::Local(path) => {
            config.asr = AsrChoice::WhisperLocal;
            config.whisper.model_path = Some(path.clone());
        }
        AsrOption::Cloud(choice) => config.asr = *choice,
    }
}

/// One entry in the unified refiner selector: no refinement, a cloud refiner,
/// or a GGUF LLM file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefineOption {
    /// No refinement -- the raw transcript (`RefineChoice::Noop`).
    None,
    /// A cloud refiner (`RefineChoice::OpenAi` or `RefineChoice::Anthropic`).
    Cloud(RefineChoice),
    /// A GGUF LLM file on disk. Selecting it sets `refine = LlamaLocal` and
    /// points `refine_settings.llama_model_path` at this file.
    Local(PathBuf),
}

impl fmt::Display for RefineOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefineOption::None => f.write_str("None (raw transcript)"),
            RefineOption::Cloud(RefineChoice::OpenAi) => f.write_str("OpenAI (cloud)"),
            RefineOption::Cloud(RefineChoice::Anthropic) => f.write_str("Anthropic (cloud)"),
            // Noop/LlamaLocal are never wrapped in `Cloud`; total match only.
            RefineOption::Cloud(_) => f.write_str("None (raw transcript)"),
            RefineOption::Local(path) => write!(f, "{} · local", local_label(path)),
        }
    }
}

/// The unified refiner selector's entries: "None" and the cloud refiners
/// first, then every GGUF LLM discovered on disk. As with [`asr_options`], a
/// configured local model outside the scan is appended so it still shows as
/// selected.
pub fn refine_options(models: &ScanResult, config: &Config) -> Vec<RefineOption> {
    let mut options = vec![
        RefineOption::None,
        RefineOption::Cloud(RefineChoice::OpenAi),
        RefineOption::Cloud(RefineChoice::Anthropic),
    ];
    options.extend(models.llm.iter().map(|m| RefineOption::Local(m.path.clone())));
    if let Some(selected) = selected_refine(config) {
        if !options.contains(&selected) {
            options.push(selected);
        }
    }
    options
}

/// The entry the refiner `pick_list` should show as selected for the current
/// config. Always `Some` for the cloud/none choices; `None` only for
/// `LlamaLocal` with no model path set yet.
pub fn selected_refine(config: &Config) -> Option<RefineOption> {
    match config.refine {
        RefineChoice::Noop => Some(RefineOption::None),
        RefineChoice::OpenAi => Some(RefineOption::Cloud(RefineChoice::OpenAi)),
        RefineChoice::Anthropic => Some(RefineOption::Cloud(RefineChoice::Anthropic)),
        RefineChoice::LlamaLocal => config
            .refine_settings
            .llama_model_path
            .clone()
            .map(RefineOption::Local),
    }
}

/// Writes a refiner selection into `config`: "None" -> `Noop`, a cloud entry
/// -> that backend, a local file -> `LlamaLocal` with the model path set. The
/// caller persists afterward.
pub fn apply_refine(config: &mut Config, option: &RefineOption) {
    match option {
        RefineOption::None => config.refine = RefineChoice::Noop,
        RefineOption::Cloud(choice) => config.refine = *choice,
        RefineOption::Local(path) => {
            config.refine = RefineChoice::LlamaLocal;
            config.refine_settings.llama_model_path = Some(path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_hf::{LocalModel, ModelKind};

    fn asr_scan(paths: &[&str]) -> ScanResult {
        ScanResult {
            asr: paths
                .iter()
                .map(|p| LocalModel {
                    filename: Path::new(p)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    path: PathBuf::from(p),
                    size_bytes: 1,
                    kind: ModelKind::Asr,
                    known_id: None,
                })
                .collect(),
            llm: Vec::new(),
        }
    }

    #[test]
    fn apply_asr_local_selects_whisper_local_and_roundtrips() {
        let mut config = Config::default();
        let option = AsrOption::Local(PathBuf::from("/models/ggml-base.bin"));
        apply_asr(&mut config, &option);

        assert_eq!(config.asr, AsrChoice::WhisperLocal);
        assert_eq!(
            config.whisper.model_path,
            Some(PathBuf::from("/models/ggml-base.bin"))
        );
        assert_eq!(selected_asr(&config), Some(option));
    }

    #[test]
    fn apply_asr_cloud_switches_backend_without_touching_model_path() {
        let mut config = Config::default();
        config.whisper.model_path = Some(PathBuf::from("/models/ggml-base.bin"));
        apply_asr(&mut config, &AsrOption::Cloud(AsrChoice::OpenAi));

        assert_eq!(config.asr, AsrChoice::OpenAi);
        // The path is preserved so switching back to local remembers it.
        assert_eq!(
            config.whisper.model_path,
            Some(PathBuf::from("/models/ggml-base.bin"))
        );
        assert_eq!(
            selected_asr(&config),
            Some(AsrOption::Cloud(AsrChoice::OpenAi))
        );
    }

    #[test]
    fn asr_options_lists_cloud_then_local_and_appends_offscan_selection() {
        let models = asr_scan(&["/models/ggml-base.bin"]);
        let mut config = Config::default();
        config.asr = AsrChoice::WhisperLocal;
        config.whisper.model_path = Some(PathBuf::from("/elsewhere/ggml-small.bin"));

        let options = asr_options(&models, &config);
        assert_eq!(options[0], AsrOption::Cloud(AsrChoice::OpenAi));
        assert_eq!(options[1], AsrOption::Cloud(AsrChoice::Deepgram));
        assert!(options.contains(&AsrOption::Local(PathBuf::from("/models/ggml-base.bin"))));
        // The configured model lives outside the scan, so it's appended.
        assert!(options.contains(&AsrOption::Local(PathBuf::from("/elsewhere/ggml-small.bin"))));
    }

    #[test]
    fn apply_refine_none_and_local_roundtrip() {
        let mut config = Config::default();
        apply_refine(&mut config, &RefineOption::None);
        assert_eq!(config.refine, RefineChoice::Noop);
        assert_eq!(selected_refine(&config), Some(RefineOption::None));

        let option = RefineOption::Local(PathBuf::from("/models/qwen.gguf"));
        apply_refine(&mut config, &option);
        assert_eq!(config.refine, RefineChoice::LlamaLocal);
        assert_eq!(
            config.refine_settings.llama_model_path,
            Some(PathBuf::from("/models/qwen.gguf"))
        );
        assert_eq!(selected_refine(&config), Some(option));
    }

    #[test]
    fn refine_options_always_offers_none_and_both_cloud_refiners() {
        let options = refine_options(&ScanResult::default(), &Config::default());
        assert_eq!(options[0], RefineOption::None);
        assert!(options.contains(&RefineOption::Cloud(RefineChoice::OpenAi)));
        assert!(options.contains(&RefineOption::Cloud(RefineChoice::Anthropic)));
    }

    #[test]
    fn option_display_labels_are_distinct() {
        assert_ne!(
            AsrOption::Cloud(AsrChoice::OpenAi).to_string(),
            AsrOption::Cloud(AsrChoice::Deepgram).to_string()
        );
        assert_ne!(
            RefineOption::None.to_string(),
            RefineOption::Cloud(RefineChoice::OpenAi).to_string()
        );
        assert!(AsrOption::Local(PathBuf::from("/x/ggml-base.bin"))
            .to_string()
            .contains("local"));
    }
}
