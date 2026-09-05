//! ASR backend implementations. Everything here implements
//! `whspr_core::AsrBackend`; the pipeline never knows or cares which one it
//! got. Bodies are `todo!()` until the ASR team wires up the real
//! dependencies (whisper-rs, HTTP clients, ...) — this crate must keep
//! compiling without those heavy deps in the meantime.

use async_trait::async_trait;
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;

use whspr_core::{AsrBackend, AsrOptions, AudioBuffer, Result, Transcript, WhsprError};

#[cfg(feature = "testkit")]
pub use whspr_core::testkit::MockAsr;


/// Cloud transcription via the OpenAI API (e.g. `whisper-1` / `gpt-4o-transcribe`).
pub struct OpenAiAsr {
    pub api_key: String,
    base_url: String,
}

impl OpenAiAsr {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com".to_string(),
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

/// OpenAI transcription API response: `{"text": "..."}`
#[derive(Debug, Deserialize)]
struct OpenAiTranscriptionResponse {
    text: String,
}

/// Encode f32 samples to WAV format in memory.
fn encode_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let mut wav_data = Vec::new();

    // WAV header: 44 bytes total
    let num_samples = samples.len() as u32;
    let sample_rate = 16000u32;
    let num_channels = 1u16;
    let bytes_per_sample = 2u16; // i16
    let byte_rate = sample_rate * num_channels as u32 * bytes_per_sample as u32;
    let block_align = num_channels * bytes_per_sample;
    let subchunk2_size = num_samples * bytes_per_sample as u32;
    let chunk_size = 36 + subchunk2_size;

    // RIFF header
    wav_data
        .write_all(b"RIFF")
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&chunk_size.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(b"WAVE")
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;

    // fmt subchunk
    wav_data
        .write_all(b"fmt ")
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&16u32.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&1u16.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?; // PCM
    wav_data
        .write_all(&num_channels.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&sample_rate.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&byte_rate.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&block_align.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&16u16.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?; // bits per sample

    // data subchunk
    wav_data
        .write_all(b"data")
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    wav_data
        .write_all(&subchunk2_size.to_le_bytes())
        .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;

    // Convert f32 samples to i16 PCM
    for &sample in samples {
        let i16_sample = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        wav_data
            .write_all(&i16_sample.to_le_bytes())
            .map_err(|e| WhsprError::Asr(format!("WAV write failed: {}", e)))?;
    }

    Ok(wav_data)
}

#[async_trait]
impl AsrBackend for OpenAiAsr {
    async fn transcribe(&self, audio: &AudioBuffer, _opts: &AsrOptions) -> Result<Transcript> {
        let wav_data = encode_wav(&audio.samples)?;

        let client = reqwest::Client::new();
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(wav_data)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .map_err(|e| WhsprError::Asr(format!("MIME type error: {}", e)))?,
            )
            .text("model", "whisper-1");

        let url = format!("{}/v1/audio/transcriptions", self.base_url);
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| WhsprError::Asr(format!("OpenAI API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(no body)".to_string());
            return Err(WhsprError::Asr(format!(
                "OpenAI API returned {}: {}",
                status, body
            )));
        }

        let api_response: OpenAiTranscriptionResponse = response
            .json()
            .await
            .map_err(|e| WhsprError::Asr(format!("Failed to parse OpenAI response: {}", e)))?;

        Ok(Transcript {
            text: api_response.text,
            language: None,
            segments: vec![],
        })
    }

    fn id(&self) -> &'static str {
        "openai"
    }
}

