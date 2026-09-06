//! Local cleanup via a small llama.cpp model (llama-cpp-2). Loads a GGUF
//! model straight off disk and runs it against the same cleanup prompt the
//! cloud refiners use - no network calls involved.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::TokenToStringError;

use whspr_core::{RefineContext, Result, TextRefiner, WhsprError};

use crate::{build_cleanup_prompt, tokens::strip_special_tokens};

/// Context window handed to the model: large enough for a normal dictation
/// utterance plus the fixed instruction text wrapped around it.
const N_CTX: u32 = 2048;

/// Hard cap on generated tokens. Cleaned-up text is never meaningfully
/// longer than the raw input, so this just bounds worst-case latency for a
/// model that never emits an end-of-generation token.
const MAX_NEW_TOKENS: i32 = 512;

/// `LlamaBackend::init()` can only succeed once per process - a second call
/// errors - and dropping a `LlamaBackend` frees the native library outright.
/// So exactly one lives here for the whole process (this `static`, like any
/// other, is never dropped at program exit) instead of one being created
/// and torn down per `refine()` call.
static BACKEND: Mutex<Option<LlamaBackend>> = Mutex::new(None);

/// Local cleanup via a small llama.cpp model. Unlike `OpenAiRefiner`/
/// `AnthropicRefiner`, nothing leaves the machine.
pub struct LlamaLocal {
    model_path: PathBuf,
}

impl LlamaLocal {
    /// `model_path` should point to a GGUF model file. Construction never
    /// fails - the path is only checked, and the model only loaded, when
    /// `refine` actually runs (see `generate` below for what happens if
    /// it's missing or invalid).
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }
}

#[async_trait]
impl TextRefiner for LlamaLocal {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        let model_path = self.model_path.clone();
        let prompt = build_cleanup_prompt(raw, ctx);

        // llama-cpp-2 is a synchronous, CPU-bound C++ library under the
        // hood, unlike the cloud refiners' awaited HTTP calls - run it on a
        // blocking thread so it doesn't stall the async runtime.
        tokio::task::spawn_blocking(move || generate(&model_path, &prompt))
            .await
            .map_err(|e| WhsprError::Refine(format!("llama-local task panicked: {e}")))?
    }

    fn id(&self) -> &'static str {
        "llama-local"
    }
}

/// Loads `model_path`, runs `prompt` through it with greedy sampling, and
/// returns the generated text. Synchronous and CPU-bound - call from a
/// blocking context. Greedy (not random) sampling is deliberate: this is a
/// cleanup pass, not creative generation, so deterministic output is the
/// right default.
fn generate(model_path: &Path, prompt: &str) -> Result<String> {
    if !model_path.is_file() {
        return Err(WhsprError::Refine(format!(
            "llama-local model not found at {}",
            model_path.display()
        )));
    }

    let mut backend_guard = BACKEND
        .lock()
        .map_err(|_| WhsprError::Refine("llama backend lock poisoned".to_string()))?;
    if backend_guard.is_none() {
        let backend = LlamaBackend::init()
            .map_err(|e| WhsprError::Refine(format!("failed to init llama backend: {e}")))?;
        *backend_guard = Some(backend);
    }
    let backend = backend_guard.as_ref().expect("just initialized above");

    let model = LlamaModel::load_from_file(backend, model_path, &LlamaModelParams::default())
        .map_err(|e| {
            WhsprError::Refine(format!(
                "failed to load llama model {}: {e}",
                model_path.display()
            ))
        })?;

    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
    let mut llama_ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| WhsprError::Refine(format!("failed to create llama context: {e}")))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| WhsprError::Refine(format!("failed to tokenize prompt: {e}")))?;

    let n_ctx = llama_ctx.n_ctx();
    let n_kv_req = tokens.len() as u32 + MAX_NEW_TOKENS as u32;
    if n_kv_req > n_ctx {
        return Err(WhsprError::Refine(format!(
            "prompt too long for llama-local's context window ({} prompt tokens + {} generation \
             budget > {} n_ctx)",
            tokens.len(),
            MAX_NEW_TOKENS,
            n_ctx
        )));
    }

    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    let last_index = (tokens.len() - 1) as i32;
    for (i, token) in (0_i32..).zip(tokens) {
        batch
            .add(token, i, &[0], i == last_index)
            .map_err(|e| WhsprError::Refine(format!("failed to build llama batch: {e}")))?;
    }
    llama_ctx
        .decode(&mut batch)
        .map_err(|e| WhsprError::Refine(format!("llama decode failed: {e}")))?;

    let mut sampler = LlamaSampler::chain_simple([LlamaSampler::greedy()]);
    let mut output = String::new();

    for n_cur in (batch.n_tokens()..).take(MAX_NEW_TOKENS as usize) {
        let token = sampler.sample(&llama_ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }

        output.push_str(&token_to_string(&model, token)?);

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| WhsprError::Refine(format!("failed to build llama batch: {e}")))?;

        llama_ctx
            .decode(&mut batch)
            .map_err(|e| WhsprError::Refine(format!("llama decode failed: {e}")))?;
    }

    Ok(strip_special_tokens(&output))
}

/// Decodes a single generated token to text. Sidesteps needing our own
/// `encoding_rs` dependency (that's what llama-cpp-2's own now-deprecated
/// `token_to_str`/`token_to_piece` helpers use internally for a proper
/// streaming decode) by reading the raw bytes and decoding them losslessly.
/// Model output isn't a trusted input, so a malformed token shouldn't be
/// able to panic this on invalid UTF-8.
fn token_to_string(model: &LlamaModel, token: LlamaToken) -> Result<String> {
    let bytes = match model.token_to_piece_bytes(token, 8, true, None) {
        Ok(bytes) => bytes,
        Err(TokenToStringError::InsufficientBufferSpace(needed)) => model
            .token_to_piece_bytes(token, usize::try_from(-needed).unwrap_or(8), true, None)
            .map_err(|e| WhsprError::Refine(format!("failed to decode llama token: {e}")))?,
        Err(e) => {
            return Err(WhsprError::Refine(format!(
                "failed to decode llama token: {e}"
            )))
        }
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
