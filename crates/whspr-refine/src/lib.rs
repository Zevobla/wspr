//! Text refiner implementations. Everything here implements
//! `whspr_core::TextRefiner`. `NoopRefiner` is real and always available
//! (it's the "no LLM cleanup" choice, not just a test double). `OpenAiRefiner`
//! and `AnthropicRefiner` are real, cloud-backed implementations.
//! `LlamaLocal` (in `llama_local.rs`) is real too, but local: it runs a GGUF
//! model through llama-cpp-2 instead of calling out to an API.
//! `NormalizingRefiner` (in `normalize/`) wraps any of the above and applies
//! rule-based number/date/time normalization to its output.

mod llama_local;
mod normalize;
mod tokens;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use llama_local::LlamaLocal;
pub use normalize::NormalizingRefiner;
use tokens::strip_special_tokens;
use whspr_core::{RefineContext, Result, TextRefiner, WhsprError};

/// Builds the shared "clean up speech-to-text" instructions used as the
/// prompt body for every refiner backend (cloud or local), so the same
/// cleanup rules apply regardless of which LLM ends up executing them.
pub(crate) fn build_cleanup_prompt(raw: &str, ctx: &RefineContext) -> String {
    let mut prompt = String::from(
        "You are a text cleanup assistant. Your job is to clean up raw speech-to-text output. \
        You must:\n\
        - Remove filler words and disfluencies (um, uh, like, you know, etc.)\n\
        - Resolve spoken self-corrections by keeping only the corrected version (e.g., 'call John, I mean Jane' -> 'call Jane')\n\
        - Add proper punctuation and capitalization\n\
        - Preserve the speaker's actual meaning and wording — do NOT paraphrase or summarize\n\
        - Output ONLY the cleaned text, nothing else (no preamble, no quotes)\n"
    );

    if let Some(ref app_name) = ctx.app_name {
        prompt.push_str(&format!(
            "\nNote: This text is being dictated into {}. Format accordingly.\n",
            app_name
        ));
    }

    if let Some(ref instructions) = ctx.instructions {
        prompt.push_str(&format!(
            "\nAdditional formatting instructions: {}\n",
            instructions
        ));
    }

    if let Some(ref prior_text) = ctx.prior_text {
        prompt.push_str(&format!(
            "\nPrior text for context (do NOT include this in your output): {}\n",
            prior_text
        ));
    }

    prompt.push_str(&format!("\nRaw text to clean up:\n{}", raw));
    prompt
}

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
    base_url: String,
}

impl OpenAiRefiner {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.openai.com".to_string(),
        }
    }

    /// For testing: override the API base URL.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiRequestMessage>,
}

#[derive(Debug, Serialize)]
struct OpenAiRequestMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    // Only `content` is consumed; serde ignores the response's other keys
    // (e.g. `role`) by default, so they are simply not declared here.
    content: String,
}

#[async_trait]
impl TextRefiner for OpenAiRefiner {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        let cleanup_prompt = build_cleanup_prompt(raw, ctx);

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages: vec![OpenAiRequestMessage {
                role: "user".to_string(),
                content: cleanup_prompt,
            }],
        };

        let client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| WhsprError::Refine(format!("OpenAI request failed: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| WhsprError::Refine(format!("Failed to read OpenAI response: {}", e)))?;

        let parsed: OpenAiResponse = serde_json::from_str(&response_text)
            .map_err(|e| WhsprError::Refine(format!("Failed to parse OpenAI response: {}", e)))?;

        parsed
            .choices
            .first()
            .and_then(|choice| {
                let content = strip_special_tokens(&choice.message.content);
                if content.is_empty() {
                    None
                } else {
                    Some(content)
                }
            })
            .ok_or_else(|| {
                WhsprError::Refine("OpenAI response had no choices or empty content".to_string())
            })
    }

    fn id(&self) -> &'static str {
        "openai"
    }
}

/// Cleanup via the Anthropic Messages API.
pub struct AnthropicRefiner {
    pub api_key: String,
    pub model: String,
    base_url: String,
}

impl AnthropicRefiner {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    /// For testing: override the API base URL.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicRequestMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicRequestMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    // Only the `text` block is consumed; the `type` discriminator and any
    // other keys serde encounters are ignored rather than declared.
    text: Option<String>,
}

#[async_trait]
impl TextRefiner for AnthropicRefiner {
    async fn refine(&self, raw: &str, ctx: &RefineContext) -> Result<String> {
        let cleanup_prompt = build_cleanup_prompt(raw, ctx);

        let system_message = "You are a text cleanup assistant for speech-to-text output. \
            Remove filler words, resolve self-corrections, add punctuation and capitalization. \
            Output ONLY the cleaned text, nothing else.";

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: system_message.to_string(),
            messages: vec![AnthropicRequestMessage {
                role: "user".to_string(),
                content: cleanup_prompt,
            }],
        };

        let client = reqwest::Client::new();
        let url = format!("{}/v1/messages", self.base_url);

        let response = client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| WhsprError::Refine(format!("Anthropic request failed: {}", e)))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| WhsprError::Refine(format!("Failed to read Anthropic response: {}", e)))?;

        let parsed: AnthropicResponse = serde_json::from_str(&response_text).map_err(|e| {
            WhsprError::Refine(format!("Failed to parse Anthropic response: {}", e))
        })?;

        parsed
            .content
            .first()
            .and_then(|c| c.text.as_ref())
            .map(|text| strip_special_tokens(text))
            .ok_or_else(|| WhsprError::Refine("Anthropic response had no text content".to_string()))
    }

    fn id(&self) -> &'static str {
        "anthropic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_openai_refiner_success() {
        let mock_server = MockServer::start().await;

        // Mock the OpenAI response
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello world."
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let refiner = OpenAiRefiner::new("test-key", "gpt-4").with_base_url(mock_server.uri());

        let result = refiner
            .refine("hello um world", &RefineContext::default())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello world.");
    }

    #[tokio::test]
    async fn test_openai_refiner_with_context() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Cleaned text with context."
                    }
                }]
            })))
            .mount(&mock_server)
            .await;

        let refiner = OpenAiRefiner::new("test-key", "gpt-4").with_base_url(mock_server.uri());

        let ctx = RefineContext {
            app_name: Some("Gmail".to_string()),
            prior_text: Some("Hi there,".to_string()),
            instructions: Some("Keep it professional".to_string()),
        };

        let result = refiner.refine("uh some text", &ctx).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Cleaned text with context.");
    }

    #[tokio::test]
    async fn test_anthropic_refiner_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "Goodbye world."
                }]
            })))
            .mount(&mock_server)
            .await;

        let refiner =
            AnthropicRefiner::new("test-key", "claude-3-sonnet").with_base_url(mock_server.uri());

        let result = refiner
            .refine("goodbye um world", &RefineContext::default())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Goodbye world.");
    }

    #[tokio::test]
    async fn test_anthropic_refiner_with_context() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "Professional response text."
                }]
            })))
            .mount(&mock_server)
            .await;

        let refiner =
            AnthropicRefiner::new("test-key", "claude-3-sonnet").with_base_url(mock_server.uri());

        let ctx = RefineContext {
            app_name: Some("Outlook".to_string()),
            prior_text: Some("Dear Sir,".to_string()),
            instructions: Some("Use formal tone".to_string()),
        };

        let result = refiner.refine("uh formal text here", &ctx).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Professional response text.");
    }

    #[test]
    fn test_noop_refiner() {
        let refiner = NoopRefiner;
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result =
            rt.block_on(refiner.refine("hello um world uh here", &RefineContext::default()));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello um world uh here");
    }

    #[test]
    fn test_noop_refiner_id() {
        assert_eq!(NoopRefiner.id(), "noop");
    }

    #[test]
    fn test_openai_refiner_id() {
        let refiner = OpenAiRefiner::new("key", "gpt-4");
        assert_eq!(refiner.id(), "openai");
    }

    #[test]
    fn test_anthropic_refiner_id() {
        let refiner = AnthropicRefiner::new("key", "claude-3");
        assert_eq!(refiner.id(), "anthropic");
    }
}
