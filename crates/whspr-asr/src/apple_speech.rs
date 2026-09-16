//! Apple on-device speech recognition backend (macOS only).
//!
//! Thin Rust wrapper over the Objective-C shim in `apple_speech.m` (compiled
//! and linked by `build.rs`). The heavy lifting — authorization, building the
//! `AVAudioPCMBuffer`, driving `SFSpeechRecognizer`'s async callbacks to a
//! final result — lives in the shim; here we just marshal the `AudioBuffer`
//! across the C ABI on a blocking worker and turn the reply into a
//! `Transcript`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float};

use async_trait::async_trait;
use whspr_core::{AsrBackend, AsrOptions, AudioBuffer, Result, Transcript, WhsprError};

extern "C" {
    fn whspr_apple_transcribe(
        samples: *const c_float,
        n_samples: usize,
        sample_rate: f64,
        locale_id: *const c_char,
        timeout_secs: f64,
        err_out: *mut *mut c_char,
    ) -> *mut c_char;
    fn whspr_apple_string_free(s: *mut c_char);
}

/// Transcription via Apple's built-in, on-device speech recognizer
/// (`SFSpeechRecognizer`). No model to download and no network: macOS ships the
/// dictation model, and recognition stays on-device whenever the locale's model
/// is installed. Best suited to short dictation clips (Apple caps a single
/// request at roughly a minute); use [`crate::WhisperLocal`] for long-form
/// audio such as imported videos.
pub struct AppleSpeech {
    /// BCP-47 locale (e.g. `"en-US"`); `None` uses the system default.
    locale: Option<String>,
}

impl AppleSpeech {
    pub fn new(locale: Option<String>) -> Self {
        Self { locale }
    }
}

/// Calls the Objective-C shim on the current (blocking) thread and turns its
/// C strings into an owned `Transcript` or a `WhsprError`.
fn transcribe_blocking(
    samples: &[f32],
    sample_rate: f64,
    locale: Option<&str>,
    timeout_secs: f64,
) -> Result<Transcript> {
    let locale_c = match locale {
        Some(l) => Some(CString::new(l).map_err(|_| {
            WhsprError::Asr("locale identifier contained an interior NUL byte".into())
        })?),
        None => None,
    };
    let locale_ptr = locale_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());

    let mut err_ptr: *mut c_char = std::ptr::null_mut();
    // SAFETY: `samples` and `locale_ptr` outlive the call and are only read by
    // the shim. It returns either a malloc'd transcript or NULL plus a malloc'd
    // error message via `err_ptr`; both are freed below before we return.
    let out = unsafe {
        whspr_apple_transcribe(
            samples.as_ptr(),
            samples.len(),
            sample_rate,
            locale_ptr,
            timeout_secs,
            &mut err_ptr,
        )
    };

    if !out.is_null() {
        // SAFETY: `out` is a valid NUL-terminated UTF-8 string from the shim.
        let text = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { whspr_apple_string_free(out) };
        return Ok(Transcript {
            text: text.trim().to_string(),
            language: locale.map(|l| l.to_string()),
            segments: Vec::new(),
        });
    }

    let msg = if err_ptr.is_null() {
        "Apple speech recognition failed".to_string()
    } else {
        // SAFETY: a non-null `err_ptr` is a valid string from the shim.
        let m = unsafe { CStr::from_ptr(err_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { whspr_apple_string_free(err_ptr) };
        m
    };
    Err(WhsprError::Asr(msg))
}

#[async_trait]
impl AsrBackend for AppleSpeech {
    async fn transcribe(&self, audio: &AudioBuffer, opts: &AsrOptions) -> Result<Transcript> {
        let samples = audio.samples.clone();
        let sample_rate = audio.sample_rate as f64;
        // An explicit per-call language wins over the configured locale.
        let locale = opts.language.clone().or_else(|| self.locale.clone());
        // Give the recognizer generous headroom over the clip length so a slow
        // first-run model load doesn't read as a hang, with a floor for very
        // short clips.
        let timeout_secs = (audio.duration_secs() as f64 * 5.0).max(30.0);

        tokio::task::spawn_blocking(move || {
            transcribe_blocking(&samples, sample_rate, locale.as_deref(), timeout_secs)
        })
        .await
        .map_err(|e| WhsprError::Asr(format!("Apple speech worker thread panicked: {e}")))?
    }

    fn id(&self) -> &'static str {
        "apple-speech"
    }
}
