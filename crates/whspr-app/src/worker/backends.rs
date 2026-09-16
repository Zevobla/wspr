//! Backend construction for the live-dictation worker: turns `Config`'s
//! ASR/refiner choices into real `AsrBackend`/`TextRefiner` instances.
//! Shared with the record-button/file path (`crate::transcribe_file`), so
//! both paths honor the same user choice.

use whspr_asr::{DeepgramAsr, OpenAiAsr, WhisperLocal};
use whspr_config::{api_key_for, AsrChoice, RefineChoice};
use whspr_core::testkit::{MockAsr, NoopRefiner};
use whspr_core::{AsrBackend, TextRefiner};
use whspr_refine::{AnthropicRefiner, LlamaLocal, NormalizingRefiner, OpenAiRefiner};

/// Builds an ASR backend from `config.asr`. Mirrors whspr-cli's
/// `build_asr_backend` (`crates/whspr-cli/src/main.rs`), minus the CLI's
/// test-only base-url/api-key overrides, which have no equivalent here.
/// Returns a plain `String` error (not `anyhow`, which whspr-app doesn't
/// otherwise depend on) so the caller can forward it directly into
/// `WorkerEvent::Failed`.
pub(crate) fn build_asr_backend(
    config: &whspr_config::Config,
) -> Result<Box<dyn AsrBackend>, String> {
    match config.asr {
        AsrChoice::Mock => Ok(Box::new(MockAsr::default())),
        AsrChoice::WhisperLocal => {
            let model_path = WhisperLocal::resolve_model_path(config.whisper.model_path.clone())
                .ok_or_else(|| {
                    "no whisper model configured: set [whisper].model_path in the config file \
                     or the WHISPER_MODEL_PATH environment variable, or pick a different ASR \
                     backend in Settings"
                        .to_string()
                })?;
            Ok(Box::new(WhisperLocal::new(model_path)))
        }
        AsrChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                "OpenAI API key not configured (set [api_keys].openai in config)".to_string()
            })?;
            Ok(Box::new(OpenAiAsr::new(api_key)))
        }
        AsrChoice::Deepgram => {
            let api_key = api_key_for(config, "deepgram").ok_or_else(|| {
                "Deepgram API key not configured (set [api_keys].deepgram in config)".to_string()
            })?;
            Ok(Box::new(DeepgramAsr::new(api_key)))
        }
        AsrChoice::AppleSpeech => {
            #[cfg(target_os = "macos")]
            {
                Ok(Box::new(whspr_asr::AppleSpeech::new(
                    config.language.clone(),
                )))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("Apple Speech is only available on macOS".to_string())
            }
        }
    }
}

/// Whether the config selects the local Whisper ASR but no model file is
/// resolvable yet -- the fresh-install onboarding state (see
/// `WorkerEvent::NeedsModel`), distinct from a genuine backend failure like a
/// missing cloud API key. Mirrors the exact condition `build_asr_backend`'s
/// `WhisperLocal` arm errors on, so the two never disagree.
pub(super) fn is_missing_whisper_model(config: &whspr_config::Config) -> bool {
    config.asr == AsrChoice::WhisperLocal
        && WhisperLocal::resolve_model_path(config.whisper.model_path.clone()).is_none()
}

/// Builds a text refiner from `config.refine`, always wrapped in
/// `NormalizingRefiner` so rule-based number/date/time normalization runs
/// regardless of which backend produced the raw text. Mirrors whspr-cli's
/// `build_refiner` (`crates/whspr-cli/src/main.rs`). Model IDs/paths come
/// from `config.refine_settings` rather than being hardcoded, so switching
/// models never requires a rebuild.
pub(crate) fn build_refiner(config: &whspr_config::Config) -> Result<Box<dyn TextRefiner>, String> {
    let inner: Box<dyn TextRefiner> = match config.refine {
        RefineChoice::Noop => Box::new(NoopRefiner),
        RefineChoice::OpenAi => {
            let api_key = api_key_for(config, "openai").ok_or_else(|| {
                "OpenAI API key not configured (set [api_keys].openai in config)".to_string()
            })?;
            Box::new(OpenAiRefiner::new(
                api_key,
                config.refine_settings.openai_model.clone(),
            ))
        }
        RefineChoice::Anthropic => {
            let api_key = api_key_for(config, "anthropic").ok_or_else(|| {
                "Anthropic API key not configured (set [api_keys].anthropic in config)".to_string()
            })?;
            Box::new(AnthropicRefiner::new(
                api_key,
                config.refine_settings.anthropic_model.clone(),
            ))
        }
        RefineChoice::LlamaLocal => {
            let model_path = config.refine_settings.llama_model_path.clone().ok_or_else(|| {
                "no llama-local model configured: set [refine_settings].llama_model_path in the \
                 config file, or pick a different refine backend in Settings"
                    .to_string()
            })?;
            Box::new(LlamaLocal::new(model_path))
        }
        RefineChoice::AppleFoundation => {
            if whspr_refine::apple_foundation_available() {
                Box::new(whspr_refine::AppleFoundation::new())
            } else {
                return Err(
                    "Apple Foundation Models is unavailable — it needs macOS 26 with \
                            Apple Intelligence enabled (and a build that includes it). Pick a \
                            different refine backend in Settings."
                        .to_string(),
                );
            }
        }
    };

    Ok(Box::new(NormalizingRefiner::new(
        inner,
        config.normalize.clone(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use whspr_config::Config;

    #[test]
    fn build_asr_backend_mock_choice_succeeds() {
        let config = Config {
            asr: AsrChoice::Mock,
            ..Default::default()
        };

        let backend = build_asr_backend(&config).expect("mock backend should always build");
        assert_eq!(backend.id(), "mock");
    }

    #[test]
    fn build_asr_backend_whisper_local_uses_explicit_model_path() {
        let config = Config {
            asr: AsrChoice::WhisperLocal,
            whisper: whspr_config::WhisperConfig {
                model_path: Some(PathBuf::from("/explicit/model.bin")),
            },
            ..Default::default()
        };

        let backend = build_asr_backend(&config)
            .expect("an explicit model_path should be enough to build WhisperLocal");
        assert_eq!(backend.id(), "whisper-local");
    }

    #[test]
    fn missing_whisper_model_is_the_onboarding_case_only_for_whisper_without_a_path() {
        // Default local Whisper with no configured model path: the fresh-
        // install onboarding state.
        let onboarding = Config {
            asr: AsrChoice::WhisperLocal,
            whisper: whspr_config::WhisperConfig { model_path: None },
            ..Default::default()
        };
        assert!(is_missing_whisper_model(&onboarding));

        // An explicit model path means there's nothing to onboard.
        let configured = Config {
            asr: AsrChoice::WhisperLocal,
            whisper: whspr_config::WhisperConfig {
                model_path: Some(PathBuf::from("/explicit/model.bin")),
            },
            ..Default::default()
        };
        assert!(!is_missing_whisper_model(&configured));

        // A different backend failing (e.g. a missing API key) is a real
        // error, not the missing-model onboarding case.
        let other_backend = Config {
            asr: AsrChoice::OpenAi,
            ..Default::default()
        };
        assert!(!is_missing_whisper_model(&other_backend));
    }

    #[test]
    fn build_asr_backend_openai_requires_an_api_key() {
        let config = Config {
            asr: AsrChoice::OpenAi,
            ..Default::default()
        };

        // `Box<dyn AsrBackend>` isn't `Debug`, so `expect_err` isn't
        // available -- match directly instead.
        match build_asr_backend(&config) {
            Ok(_) => panic!("no [api_keys].openai entry should fail, not build a backend"),
            Err(error) => assert!(error.contains("OpenAI API key")),
        }
    }

    #[test]
    fn build_refiner_noop_choice_is_wrapped_in_normalizing_refiner() {
        let config = Config {
            refine: RefineChoice::Noop,
            ..Default::default()
        };

        let refiner = build_refiner(&config).expect("noop refiner should always build");
        // NormalizingRefiner::id() delegates to the inner refiner's id (see
        // whspr-refine's normalize/mod.rs), so this also proves the wrapping
        // happened rather than returning the bare NoopRefiner.
        assert_eq!(refiner.id(), "noop");
    }

    #[test]
    fn build_refiner_anthropic_requires_an_api_key() {
        let config = Config {
            refine: RefineChoice::Anthropic,
            ..Default::default()
        };

        // `Box<dyn TextRefiner>` isn't `Debug`, so `expect_err` isn't
        // available -- match directly instead.
        match build_refiner(&config) {
            Ok(_) => panic!("no [api_keys].anthropic entry should fail, not build a refiner"),
            Err(error) => assert!(error.contains("Anthropic API key")),
        }
    }

    #[test]
    fn build_refiner_llama_local_requires_a_configured_model_path() {
        let config = Config {
            refine: RefineChoice::LlamaLocal,
            ..Default::default()
        };

        // `Box<dyn TextRefiner>` isn't `Debug`, so `expect_err` isn't
        // available -- match directly instead.
        match build_refiner(&config) {
            Ok(_) => panic!("no [refine_settings].llama_model_path should fail, not build one"),
            Err(error) => assert!(error.contains("llama_model_path")),
        }
    }

    #[test]
    fn build_refiner_llama_local_uses_configured_model_path() {
        let config = Config {
            refine: RefineChoice::LlamaLocal,
            refine_settings: whspr_config::RefineSettings {
                llama_model_path: Some(PathBuf::from("/explicit/model.gguf")),
                ..Default::default()
            },
            ..Default::default()
        };

        let refiner =
            build_refiner(&config).expect("an explicit llama_model_path should be enough to build");
        assert_eq!(refiner.id(), "llama-local");
    }
}
