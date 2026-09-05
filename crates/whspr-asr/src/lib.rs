//! ASR backend implementations. Everything here implements
//! `whspr_core::AsrBackend`; the pipeline never knows or cares which one it
//! got. `WhisperLocal` (whisper-rs), `OpenAiAsr`, and `DeepgramAsr` are all
//! real implementations now.

use async_trait::async_trait;
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;

use whspr_core::{
    AsrBackend, AsrOptions, AudioBuffer, Result, Transcript, TranscriptSegment, WhsprError,
};

/// Local transcription via whisper.cpp (whisper-rs).
///
/// `transcribe` assumes the `AudioBuffer` it receives is already 16kHz mono
/// f32 PCM, per the contract documented on `whspr_core::AudioBuffer` (capture/
/// decode/resample all normalize to that shape before anything touches an
/// `AsrBackend`) — no resampling happens in here.
pub struct WhisperLocal {
    pub model_path: PathBuf,
}

impl WhisperLocal {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
        }
    }

    /// Resolves the GGML model path to use: an explicit path (e.g. from a
    /// `--model` flag or `whspr-config`'s `[whisper].model_path`) takes
    /// priority. If none is given, falls back to the `WHISPER_MODEL_PATH`
    /// environment variable, which the project's Nix devShell sets to a
    /// pinned, reproducibly-fetched model (see `nix/models.nix`) so nobody
    /// needs to download one by hand. That's a build/environment-provided
    /// path, not a user-changeable app setting, so reading it here doesn't
    /// run afoul of `whspr-config`'s "no env vars" rule — that rule is
    /// specifically about app settings silently overriding the config file
    /// (see that crate's module doc comment).
    ///
    /// Returns `None` if neither is available; callers should surface that
    /// as a clear "no model configured" error rather than constructing a
    /// `WhisperLocal` pointed at a path that doesn't exist.
    pub fn resolve_model_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
        explicit.or_else(|| std::env::var_os("WHISPER_MODEL_PATH").map(PathBuf::from))
    }
}

#[async_trait]
impl AsrBackend for WhisperLocal {
    async fn transcribe(&self, audio: &AudioBuffer, opts: &AsrOptions) -> Result<Transcript> {
        if !self.model_path.exists() {
            return Err(WhsprError::Asr(format!(
                "WhisperLocal model file not found at {}; download a GGML model (e.g. \
                 ggml-base.bin from https://huggingface.co/ggerganov/whisper.cpp) and point \
                 `model_path` at it.",
                self.model_path.display()
            )));
        }

        let model_path = self.model_path.clone();
        let samples = audio.samples.clone();
        let language = opts.language.clone();

        // whisper.cpp inference is CPU-bound and can take real wall-clock
        // seconds; run it on a blocking-pool thread rather than blocking the
        // async runtime directly. WhisperContext/WhisperState/FullParams are
        // all `Send + Sync` (whisper-rs marks them so explicitly), so moving
        // them into the closure and running synchronously in there is sound.
        tokio::task::spawn_blocking(move || {
            transcribe_blocking(&model_path, &samples, language.as_deref())
        })
        .await
        .map_err(|e| WhsprError::Asr(format!("WhisperLocal worker thread panicked: {}", e)))?
    }

    fn id(&self) -> &'static str {
        "whisper-local"
    }
}

/// Runs whisper.cpp inference synchronously. Called from inside
/// `spawn_blocking` — never call this directly from an async context.
fn transcribe_blocking(
    model_path: &std::path::Path,
    samples: &[f32],
    language: Option<&str>,
) -> Result<Transcript> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .map_err(|e| WhsprError::Asr(format!("failed to load whisper model: {}", e)))?;
    let mut state = ctx
        .create_state()
        .map_err(|e| WhsprError::Asr(format!("failed to create whisper state: {}", e)))?;

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_language(language);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    state
        .full(params, samples)
        .map_err(|e| WhsprError::Asr(format!("whisper inference failed: {}", e)))?;

    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let text = segment
            .to_str_lossy()
            .map_err(|e| WhsprError::Asr(format!("failed to decode whisper segment: {}", e)))?;
        segments.push(TranscriptSegment {
            text: text.trim().to_string(),
            start_secs: segment.start_timestamp() as f32 / 100.0,
            end_secs: segment.end_timestamp() as f32 / 100.0,
            speaker: None,
        });
    }

    let text = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    Ok(Transcript {
        text,
        language: language.map(str::to_string),
        segments,
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

    /// Exercises all three precedence outcomes in one test (rather than
    /// three separate `#[test]` fns) since `cargo test` runs tests in
    /// parallel threads by default and mutating a shared env var from
    /// multiple concurrently-running tests would race.
    #[test]
    fn resolve_model_path_precedence() {
        std::env::remove_var("WHISPER_MODEL_PATH");
        assert_eq!(WhisperLocal::resolve_model_path(None), None);

        std::env::set_var("WHISPER_MODEL_PATH", "/from/env.bin");
        assert_eq!(
            WhisperLocal::resolve_model_path(None),
            Some(PathBuf::from("/from/env.bin")),
            "should fall back to WHISPER_MODEL_PATH when no explicit path is given"
        );
        assert_eq!(
            WhisperLocal::resolve_model_path(Some(PathBuf::from("/explicit.bin"))),
            Some(PathBuf::from("/explicit.bin")),
            "an explicit path should win over WHISPER_MODEL_PATH"
        );
        std::env::remove_var("WHISPER_MODEL_PATH");
    }

    #[tokio::test]
    async fn test_openai_asr_transcribe() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();

        // Mock the OpenAI API endpoint
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "hello world"})),
            )
            .mount(&mock_server)
            .await;

        let asr = OpenAiAsr::with_base_url("test-key", &base_url);
        let audio = AudioBuffer::new(vec![0.1; 16000], 16000); // 1 second of audio
        let opts = AsrOptions::default();

        let result = asr.transcribe(&audio, &opts).await;
        assert!(result.is_ok());
        let transcript = result.unwrap();
        assert_eq!(transcript.text, "hello world");
    }

    #[tokio::test]
    async fn test_deepgram_asr_transcribe() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();

        // Mock the Deepgram API endpoint
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1/listen"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [
                        {
                            "alternatives": [
                                {"transcript": "deepgram test"}
                            ]
                        }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let asr = DeepgramAsr::with_base_url("test-key", &base_url);
        let audio = AudioBuffer::new(vec![0.1; 16000], 16000); // 1 second of audio
        let opts = AsrOptions::default();

        let result = asr.transcribe(&audio, &opts).await;
        assert!(result.is_ok());
        let transcript = result.unwrap();
        assert_eq!(transcript.text, "deepgram test");
    }

    #[test]
    fn test_encode_wav() {
        // Create a simple test signal
        let samples = vec![0.0, 0.5, -0.5, 0.25];
        let wav_data = encode_wav(&samples).unwrap();

        // Check WAV header starts with "RIFF"
        assert!(wav_data.starts_with(b"RIFF"));
        // Check it has WAVE marker
        assert_eq!(&wav_data[8..12], b"WAVE");
        // Check it has fmt subchunk
        assert!(wav_data[12..].windows(4).any(|w| w == b"fmt "));
        // Check it has data subchunk
        assert!(wav_data.windows(4).any(|w| w == b"data"));
        // Total size should be 44 byte header + samples.len() * 2 bytes for i16 PCM data
        assert!(wav_data.len() >= 44 + samples.len() * 2);
    }

    /// Real end-to-end WhisperLocal transcription against a small committed
    /// speech fixture (`tests/fixtures/one-two-three.wav`, 16kHz mono,
    /// synthesized speech saying "one two three").
    ///
    /// Requires a real GGML model on disk. Inside `nix develop`, one is
    /// already available for free via `WHISPER_MODEL_PATH` (see
    /// `WhisperLocal::resolve_model_path`), so `cargo test -p whspr-asr --
    /// --ignored` Just Works there with no setup. Outside the devShell,
    /// fall back to a manual download:
    ///
    /// ```sh
    /// mkdir -p ~/.cache/whspr
    /// curl -L -o ~/.cache/whspr/ggml-base.bin \
    ///   https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
    /// cargo test -p whspr-asr -- --ignored
    /// ```
    #[tokio::test]
    #[ignore]
    async fn whisper_local_transcribes_real_speech_fixture() {
        let cache_fallback = PathBuf::from(std::env::var("HOME").expect("HOME not set"))
            .join(".cache/whspr/ggml-base.bin");
        let model_path = WhisperLocal::resolve_model_path(None)
            .filter(|p| p.exists())
            .or_else(|| Some(cache_fallback.clone()).filter(|p| p.exists()));
        let Some(model_path) = model_path else {
            eprintln!(
                "skipping whisper_local_transcribes_real_speech_fixture: no model found via \
                 WHISPER_MODEL_PATH (set automatically inside `nix develop`) or at {} \
                 (see this test's doc comment for the manual download command)",
                cache_fallback.display()
            );
            return;
        };

        let fixture_path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/one-two-three.wav"
        ));
        let mut reader = hound::WavReader::open(fixture_path).expect("failed to open fixture WAV");
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.expect("failed to read WAV sample") as f32 / 32768.0)
            .collect();
        let audio = AudioBuffer::new(samples, 16000);

        let asr = WhisperLocal::new(model_path);
        let opts = AsrOptions {
            language: Some("en".to_string()),
        };

        let transcript = asr
            .transcribe(&audio, &opts)
            .await
            .expect("transcription failed");

        assert!(
            !transcript.text.trim().is_empty(),
            "expected a non-empty transcript, got: {:?}",
            transcript.text
        );
    }
}
