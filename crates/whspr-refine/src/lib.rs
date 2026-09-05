//! Text refiner implementations. Everything here implements
//! `whspr_core::TextRefiner`. `NoopRefiner` is real and always available
//! (it's the "no LLM cleanup" choice, not just a test double); the LLM-backed
//! refiners are `todo!()` until the refine team wires up the real HTTP/local
//! inference calls — this crate must keep compiling without llama-cpp-2 in
//! the meantime.

use async_trait::async_trait;

use whspr_core::{RefineContext, Result, TextRefiner};

/// Passes text through unchanged. The default `RefineChoice`.
pub struct NoopRefiner;

#[async_trait]
impl TextRefiner for NoopRefiner {
    async fn refine(&self, raw: &str, _ctx: &RefineContext) -> Result<String> {
        Ok(raw.to_string())
    }

    fn id(&self) -> &'static str {
        "noop"
    }
}

/// Cleanup via the OpenAI chat/completions API.
pub struct OpenAiRefiner {
    pub api_key: String,
    pub model: String,
}

impl OpenAiRefiner {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[async_trait]
impl TextRefiner for OpenAiRefiner {
    async fn refine(&self, _raw: &str, _ctx: &RefineContext) -> Result<String> {
        todo!("whspr-refine: wire up OpenAI chat completion call")
    }

    fn id(&self) -> &'static str {
        "openai"
    }
}

/// Cleanup via the Anthropic Messages API.
pub struct AnthropicRefiner {
    pub api_key: String,
    pub model: String,
}

impl AnthropicRefiner {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[async_trait]
impl TextRefiner for AnthropicRefiner {
    async fn refine(&self, _raw: &str, _ctx: &RefineContext) -> Result<String> {
        todo!("whspr-refine: wire up Anthropic messages call")
    }

    fn id(&self) -> &'static str {
        "anthropic"
    }
}

/// Local cleanup via a small llama.cpp model (llama-cpp-2). Opt in the
/// `llama-cpp-2` workspace dep from this crate's own Cargo.toml when
/// implementing.
pub struct LlamaLocal {
    pub model_path: std::path::PathBuf,
}

impl LlamaLocal {
    pub fn new(model_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }
}

#[async_trait]
impl TextRefiner for LlamaLocal {
    async fn refine(&self, _raw: &str, _ctx: &RefineContext) -> Result<String> {
        todo!("whspr-refine: wire up llama-cpp-2 local inference")
    }

    fn id(&self) -> &'static str {
        "llama-local"
    }
}
