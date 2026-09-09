//! Reusable local-LLM inference primitive built on llama-cpp-2.
//!
//! `LocalLlm` runs an arbitrary prompt through a GGUF model on the CPU and
//! returns the generated text. It is deliberately prompt-agnostic: callers
//! (e.g. `LlamaLocal`'s cleanup path, and later phases doing lecture
//! classification or verbatim-sentence selection) build whatever prompt they
//! need and hand it in. The expensive GGUF load is cached process-wide by
//! model-file path, so repeated `generate` calls against the same model do
//! not re-read it from disk; only a fresh (cheap) context is created per call.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::TokenToStringError;

use whspr_core::{Result, WhsprError};

use crate::tokens::strip_special_tokens;

/// Context window handed to each model context: large enough for a normal
/// dictation utterance plus the fixed instruction text wrapped around it.
const N_CTX: u32 = 2048;

/// `LlamaBackend::init()` can only succeed once per process - a second call
/// errors - and dropping a `LlamaBackend` frees the native library outright.
/// So exactly one lives here for the whole process (this `static`, like any
/// other, is never dropped at program exit) instead of one being created and
/// torn down per `generate` call.
static BACKEND: Mutex<Option<LlamaBackend>> = Mutex::new(None);

/// Process-wide cache of loaded models, keyed by model-file path. Loading a
/// GGUF is the expensive step (seconds, hundreds of MB), so once a model is
/// loaded it is kept alive for the rest of the process and shared via `Arc`
/// across every `LocalLlm` that names the same path. Contexts are cheap and
/// created fresh per `generate`, so they are not cached here.
static MODEL_CACHE: Mutex<Option<HashMap<PathBuf, Arc<LlamaModel>>>> = Mutex::new(None);

/// Counts how many times a GGUF was actually read from disk (as opposed to
/// served from `MODEL_CACHE`). Only used by tests to prove caching works.
#[cfg(test)]
static MODEL_LOADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Tunables for a single `generate` call.
#[derive(Debug, Clone)]
pub struct GenOpts {
    /// Hard cap on how many tokens to generate. Bounds worst-case latency for
    /// a model that never emits an end-of-generation token.
    pub max_new_tokens: usize,
    /// Optional stop strings: generation halts (and the output is truncated)
    /// as soon as the accumulated text contains any of these. Empty means no
    /// stop strings - generate until EOG or `max_new_tokens`.
    pub stop: Vec<String>,
}

impl Default for GenOpts {
    fn default() -> Self {
        Self {
            max_new_tokens: 512,
            stop: Vec::new(),
        }
    }
}

/// A handle to a local GGUF model. Cheap to clone and to construct - the model
/// is only loaded (once, then cached) when `generate` first runs against it.
#[derive(Debug, Clone)]
pub struct LocalLlm {
    model_path: PathBuf,
}

impl LocalLlm {
    /// `model_path` should point to a GGUF model file. Construction never
    /// fails - the path is only checked, and the model only loaded, when
    /// `generate` actually runs.
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }

    /// Runs `prompt` through the model with greedy sampling and returns the
    /// generated text. Synchronous and CPU-bound - call from a blocking
    /// context (e.g. `tokio::task::spawn_blocking`). Greedy (not random)
    /// sampling is deliberate: deterministic output is the right default for
    /// the structured tasks this primitive backs.
    pub fn generate(&self, prompt: &str, opts: &GenOpts) -> Result<String> {
        if !self.model_path.is_file() {
            return Err(WhsprError::Refine(format!(
                "llama-local model not found at {}",
                self.model_path.display()
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

        let model = load_model(backend, &self.model_path)?;

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX));
        let mut llama_ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| WhsprError::Refine(format!("failed to create llama context: {e}")))?;

        let tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| WhsprError::Refine(format!("failed to tokenize prompt: {e}")))?;

        let n_ctx = llama_ctx.n_ctx();
        let n_kv_req = tokens.len() as u32 + opts.max_new_tokens as u32;
        if n_kv_req > n_ctx {
            return Err(WhsprError::Refine(format!(
                "prompt too long for llama-local's context window ({} prompt tokens + {} \
                 generation budget > {} n_ctx)",
                tokens.len(),
                opts.max_new_tokens,
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

        for n_cur in (batch.n_tokens()..).take(opts.max_new_tokens) {
            let token = sampler.sample(&llama_ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if model.is_eog_token(token) {
                break;
            }

            output.push_str(&token_to_string(&model, token)?);

            if let Some(idx) = stop_index(&output, &opts.stop) {
                output.truncate(idx);
                break;
            }

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
}

/// Returns the earliest byte index at which any non-empty stop string occurs
/// in `text`, or `None` if none is present. Used to truncate generation at a
/// stop sequence, which may span several generated tokens - hence the check
/// against the full accumulated output rather than a single token.
fn stop_index(text: &str, stops: &[String]) -> Option<usize> {
    stops
        .iter()
        .filter(|s| !s.is_empty())
        .filter_map(|s| text.find(s.as_str()))
        .min()
}

/// Fetches the model for `path` from `MODEL_CACHE`, loading it from disk (and
/// caching it) on first use. The returned `Arc` shares the single cached
/// instance - repeated calls for the same path never re-read the GGUF.
fn load_model(backend: &LlamaBackend, path: &Path) -> Result<Arc<LlamaModel>> {
    let mut cache = MODEL_CACHE
        .lock()
        .map_err(|_| WhsprError::Refine("llama model cache lock poisoned".to_string()))?;
    let map = cache.get_or_insert_with(HashMap::new);

    if let Some(model) = map.get(path) {
        return Ok(Arc::clone(model));
    }

    let model =
        LlamaModel::load_from_file(backend, path, &LlamaModelParams::default()).map_err(|e| {
            WhsprError::Refine(format!(
                "failed to load llama model {}: {e}",
                path.display()
            ))
        })?;

    #[cfg(test)]
    MODEL_LOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let model = Arc::new(model);
    map.insert(path.to_path_buf(), Arc::clone(&model));
    Ok(model)
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
    fn gen_opts_default_values() {
        let opts = GenOpts::default();
        assert_eq!(opts.max_new_tokens, 512);
        assert!(opts.stop.is_empty());
    }

    #[test]
    fn stop_index_none_when_no_stops() {
        assert_eq!(stop_index("hello world", &[]), None);
    }

    #[test]
    fn stop_index_ignores_empty_stop_strings() {
        assert_eq!(stop_index("hello world", &[String::new()]), None);
    }

    #[test]
    fn stop_index_finds_earliest_stop() {
        let stops = vec!["\n\n".to_string(), "END".to_string()];
        // "END" is at byte 6; the "\n\n" isn't present.
        assert_eq!(stop_index("hello END there", &stops), Some(6));
    }

    #[test]
    fn stop_index_picks_the_first_occurring_of_several() {
        let stops = vec!["world".to_string(), "hello".to_string()];
        assert_eq!(stop_index("say hello then world", &stops), Some(4));
    }

    #[test]
    fn missing_model_errors_without_loading() {
        let llm = LocalLlm::new("/definitely/does/not/exist.gguf");
        let result = llm.generate("hi", &GenOpts::default());
        assert!(result.is_err());
    }

    /// Real end-to-end run against a real GGUF model. Ignored by default so
    /// the offline gate stays green without a model file on disk; point
    /// `WHSPR_LLAMA_TEST_MODEL` at a small GGUF (e.g. a SmolLM2-135M-Instruct
    /// quant) and run with `cargo test -p whspr-refine -- --ignored` to
    /// exercise it for real. Also asserts the GGUF is loaded exactly once
    /// across two `generate` calls, proving the model cache works.
    #[test]
    #[ignore]
    fn real_model_loads_once_across_calls() {
        let Ok(model_path) = std::env::var("WHSPR_LLAMA_TEST_MODEL") else {
            eprintln!("skipping: WHSPR_LLAMA_TEST_MODEL not set");
            return;
        };

        let llm = LocalLlm::new(model_path);
        let opts = GenOpts {
            max_new_tokens: 16,
            stop: Vec::new(),
        };

        let first = llm
            .generate("Say hello.", &opts)
            .expect("first generate should succeed against a real model");
        let loads_after_first = MODEL_LOADS.load(std::sync::atomic::Ordering::Relaxed);

        let _second = llm
            .generate("Say goodbye.", &opts)
            .expect("second generate should succeed against a real model");
        let loads_after_second = MODEL_LOADS.load(std::sync::atomic::Ordering::Relaxed);

        assert!(!first.is_empty());
        // The second call must reuse the cached model, not re-read the GGUF.
        assert_eq!(loads_after_first, loads_after_second);
    }
}
