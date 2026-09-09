//! Local cleanup via a small llama.cpp model. Builds the shared cleanup
//! prompt and runs it through the reusable `LocalLlm` inference primitive
//! (in `local_llm.rs`) - no network calls involved. Unlike `OpenAiRefiner`/
//! `AnthropicRefiner`, nothing leaves the machine.

use std::path::PathBuf;

use async_trait::async_trait;

use whspr_core::{RefineContext, Result, TextRefiner, WhsprError};

use crate::build_cleanup_prompt;
use crate::local_llm::{GenOpts, LocalLlm};

/// Local cleanup via a small llama.cpp model. Unlike `OpenAiRefiner`/
/// `AnthropicRefiner`, nothing leaves the machine.
pub struct LlamaLocal {
    llm: LocalLlm,
}

impl LlamaLocal {
    /// `model_path` should point to a GGUF model file. Construction never
    /// fails - the path is only checked, and the model only loaded, when
    /// `refine` actually runs (if it's missing or invalid, `refine` returns
    /// an error rather than panicking).
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            llm: LocalLlm::new(model_path),
        }
    }
}

#[async_trait]
impl TextRefiner for LlamaLocal {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        let llm = self.llm.clone();
        let prompt = build_cleanup_prompt(raw, ctx);

        // The primitive is synchronous and CPU-bound (a C++ library under the
        // hood), unlike the cloud refiners' awaited HTTP calls - run it on a
        // blocking thread so it doesn't stall the async runtime.
        tokio::task::spawn_blocking(move || llm.generate(&prompt, &GenOpts::default()))
            .await
            .map_err(|e| WhsprError::Refine(format!("llama-local task panicked: {e}")))?
    }

    fn id(&self) -> &'static str {
        "llama-local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama_local_id() {
        let refiner = LlamaLocal::new("/path/to/model.gguf");
        assert_eq!(refiner.id(), "llama-local");
    }

    #[tokio::test]
    async fn test_llama_local_missing_model_errors_no_panic() {
        let refiner = LlamaLocal::new("/definitely/does/not/exist.gguf");

        let result = refiner
            .refine("hello um world", &RefineContext::default())
            .await;

        assert!(result.is_err());
    }

    /// Real end-to-end run against a real GGUF model. Ignored by default so
    /// the offline gate stays green without a model file on disk; point
    /// `WHSPR_LLAMA_TEST_MODEL` at a small GGUF (e.g. a SmolLM2-135M-Instruct
    /// quant) and run with `cargo test -p whspr-refine -- --ignored` to
    /// exercise it for real.
    #[tokio::test]
    #[ignore]
    async fn test_llama_local_real_model() {
        let Ok(model_path) = std::env::var("WHSPR_LLAMA_TEST_MODEL") else {
            eprintln!("skipping: WHSPR_LLAMA_TEST_MODEL not set");
            return;
        };

        let refiner = LlamaLocal::new(model_path);
        let result = refiner
            .refine(
                "um so i think we should uh meet on tuesday, I mean Wednesday",
                &RefineContext::default(),
            )
            .await
            .expect("refine should succeed against a real model");

        assert!(!result.is_empty());
    }
}
