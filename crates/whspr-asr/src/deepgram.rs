//! Cloud transcription via the Deepgram API. Split out of `lib.rs` to keep
//! that file under this project's 600-line-per-file guideline (AA-06).

use async_trait::async_trait;
use serde::Deserialize;

use whspr_core::{AsrBackend, AsrOptions, AudioBuffer, Result, Transcript, WhsprError};

use crate::encode_wav;

/// Cloud transcription via the Deepgram API.
pub struct DeepgramAsr {
    pub api_key: String,
    base_url: String,
}

impl DeepgramAsr {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.deepgram.com".to_string(),
        }
    }

    /// Create an instance with a custom base URL (useful for testing with wiremock).
    #[doc(hidden)]
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }
}

/// Deepgram response structure: `results.channels[0].alternatives[0].transcript`
#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Deserialize)]
struct DeepgramTranscriptionResponse {
    results: DeepgramResults,
}

#[async_trait]
impl AsrBackend for DeepgramAsr {
    async fn transcribe(&self, audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        let wav_data = encode_wav(&audio.samples)?;

        let client = reqwest::Client::new();
        let url = format!("{}/v1/listen", self.base_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .body(wav_data)
            .send()
            .await
            .map_err(|e| WhsprError::Asr(format!("Deepgram API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(no body)".to_string());
            return Err(WhsprError::Asr(format!(
                "Deepgram API returned {}: {}",
                status, body
            )));
        }

        let api_response: DeepgramTranscriptionResponse = response
            .json()
            .await
            .map_err(|e| WhsprError::Asr(format!("Failed to parse Deepgram response: {}", e)))?;

        let text = api_response
            .results
            .channels
            .first()
            .and_then(|ch| ch.alternatives.first())
            .map(|alt| alt.transcript.clone())
            .unwrap_or_default();

        Ok(Transcript {
            text,
            language: None,
            segments: vec![],
        })
    }

    fn id(&self) -> &'static str {
        "deepgram"
    }
}
