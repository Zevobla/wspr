//! Transcribe audio fetched from a media URL (YouTube, a podcast, a lecture
//! link) straight to on-screen text for the Dictate screen's "Transcribe from
//! URL..." action. This is the file-transcribe flow with a different front
//! door: `whspr_import::download_to_audio` shells out to yt-dlp + ffmpeg to
//! produce a 16kHz-mono `AudioBuffer`, then the *same* pipeline the file
//! button and record button use (`crate::transcribe_file::run_transcribe_audio`)
//! turns it into text. The result is routed back through
//! `Message::FileTranscribed`, so on-screen display, history, and speaker
//! attribution are all reused verbatim.

use whspr_config::Config;

use crate::transcribe_file::{run_transcribe_audio, TranscribeOutcome};

/// Downloads the audio-only stream at `url` (anonymously, whole clip),
/// transcodes it to 16kHz mono, transcribes + refines it with `config`'s
/// backends, then best-effort deletes the temp WAV `whspr-import` handed back
/// (the caller owns that file -- see `whspr_import`'s temp-file policy).
///
/// Runs entirely off the UI thread via `Task::perform`. Every failure -- a
/// missing yt-dlp/ffmpeg, a bad URL, a download or inference error -- comes
/// back as a `String`, never a panic, so it lands in `Message::FileTranscribed(Err(..))`
/// and surfaces in the Dictate screen's status line. When the error is a
/// missing-tool one, a short install hint is prepended so the message reads
/// legibly to a user who hasn't set the tools up yet.
pub async fn run_transcribe_url(url: String, config: Config) -> Result<TranscribeOutcome, String> {
    let (wav, audio) =
        whspr_import::download_to_audio(&url, None, whspr_import::CookiesFrom::None)
            .await
            .map_err(|e| with_install_hint(e.to_string()))?;
    let outcome = run_transcribe_audio(audio, config).await;
    // Best-effort temp cleanup: `whspr-import` hands ownership of the WAV to
    // us and never reaps it. A failed delete isn't worth failing an
    // otherwise-good transcription over, so the result is ignored.
    let _ = std::fs::remove_file(&wav);
    outcome
}

/// Prepends a concrete install hint when `error` is a missing-tool error from
/// `whspr-import` (its yt-dlp/ffmpeg lookups report "... not found: ..."), so
/// the status line tells a first-time user exactly what to do. Any other
/// error is returned unchanged.
fn with_install_hint(error: String) -> String {
    if error.contains("not found") {
        format!("Install yt-dlp and ffmpeg (e.g. brew install yt-dlp ffmpeg). {error}")
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::with_install_hint;

    #[test]
    fn install_hint_is_prepended_for_missing_tools() {
        let hinted = with_install_hint("yt-dlp not found: install it".to_string());
        assert!(hinted.starts_with("Install yt-dlp and ffmpeg"));
        assert!(hinted.contains("yt-dlp not found"));
    }

    #[test]
    fn other_errors_pass_through_unchanged() {
        let msg = "yt-dlp audio download failed: HTTP 403".to_string();
        assert_eq!(with_install_hint(msg.clone()), msg);
    }
}
