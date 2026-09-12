//! Downloading a URL's audio to a 16kHz mono WAV via `yt-dlp` + `ffmpeg`.
//!
//! [`download_audio`] pulls the audio-only stream (`-f bestaudio
//! --extract-audio`) and has yt-dlp's bundled ffmpeg transcode it to a 16kHz
//! mono WAV, returning that file's path. [`download_to_audio`] then runs it
//! through `whspr-audio`'s decode/resample to yield an
//! [`whspr_core::AudioBuffer`] whisper can consume directly.
//!
//! **The returned WAV lives in a temp directory and is owned by the caller,
//! which must delete it after transcribing.** This library never reaps it;
//! the delete-after-use policy is enforced one layer up, in the app.

use std::path::PathBuf;

use whspr_core::{AudioBuffer, Result, WhsprError};

use crate::resolve::{Chapter, CookiesFrom};
use crate::tools::{resolve_tool, Tool};

/// A half-open time window to import, in seconds. Built from a user
/// selection or from a [`Chapter`] (see [`ClipRange::from_chapter`]), and
/// rendered to a `yt-dlp --download-sections` value by
/// [`ClipRange::to_download_section`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRange {
    pub start_secs: f32,
    pub end_secs: f32,
}

impl ClipRange {
    pub fn new(start_secs: f32, end_secs: f32) -> Self {
        Self {
            start_secs,
            end_secs,
        }
    }

    /// The clip covering exactly one chapter — how the app imports "just this
    /// section" from a chapter heading.
    pub fn from_chapter(chapter: &Chapter) -> Self {
        Self {
            start_secs: chapter.start_secs,
            end_secs: chapter.end_secs,
        }
    }

    /// yt-dlp `--download-sections` syntax: `*START-END` in seconds (the `*`
    /// marks a time range rather than a chapter-title regex).
    pub fn to_download_section(&self) -> String {
        format!("*{}-{}", self.start_secs, self.end_secs)
    }
}

/// Downloads `url`'s audio to a fresh 16kHz mono WAV and returns its path.
///
/// With `clip`, only that section is fetched (`--download-sections`). With
/// [`CookiesFrom::Browser`], authenticates via `--cookies-from-browser`.
/// The caller owns and must delete the returned file (see the module docs).
pub async fn download_audio(
    url: &str,
    clip: Option<ClipRange>,
    cookies: CookiesFrom,
) -> Result<PathBuf> {
    let ytdlp = resolve_tool(Tool::YtDlp).ok_or_else(|| {
        WhsprError::Other("yt-dlp not found: install it or point WHSPR_YTDLP at it".to_string())
    })?;
    let ffmpeg = resolve_tool(Tool::Ffmpeg).ok_or_else(|| {
        WhsprError::Other("ffmpeg not found: install it or point WHSPR_FFMPEG at it".to_string())
    })?;

    let dir = unique_temp_dir()?;
    let out_template = dir.join("audio.%(ext)s");
    let wav_path = dir.join("audio.wav");

    let mut cmd = tokio::process::Command::new(&ytdlp);
    cmd.arg("-f")
        .arg("bestaudio")
        .arg("--extract-audio")
        .arg("--audio-format")
        .arg("wav")
        // Force the extract-audio ffmpeg pass to emit 16kHz mono directly.
        .arg("--postprocessor-args")
        .arg("ExtractAudio:-ar 16000 -ac 1")
        .arg("--ffmpeg-location")
        .arg(&ffmpeg)
        .arg("-o")
        .arg(&out_template);
    if let Some(clip) = &clip {
        cmd.arg("--download-sections")
            .arg(clip.to_download_section());
    }
    if let CookiesFrom::Browser(browser) = &cookies {
        cmd.arg("--cookies-from-browser").arg(browser);
    }
    cmd.arg(url);

    let output = cmd
        .output()
        .await
        .map_err(|e| WhsprError::Other(format!("failed to run yt-dlp: {e}")))?;
    if !output.status.success() {
        return Err(WhsprError::Other(format!(
            "yt-dlp audio download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if !wav_path.exists() {
        return Err(WhsprError::Other(format!(
            "yt-dlp reported success but no WAV was produced at {}",
            wav_path.display()
        )));
    }
    Ok(wav_path)
}

/// [`download_audio`] plus decode/resample: returns the WAV path (for the
/// caller to delete) and the 16kHz mono [`AudioBuffer`] ready for whisper.
pub async fn download_to_audio(
    url: &str,
    clip: Option<ClipRange>,
    cookies: CookiesFrom,
) -> Result<(PathBuf, AudioBuffer)> {
    let wav = download_audio(url, clip, cookies).await?;
    let decoded = whspr_audio::decode_wav(&wav)?;
    let audio = whspr_audio::resample_to_16k_mono(&decoded)?;
    Ok((wav, audio))
}

/// Fetches a thumbnail image straight from its URL (a public CDN image from
/// [`crate::MediaInfo`]'s `thumbnail`) and returns its raw bytes. A plain HTTP
/// GET -- fast, and no second yt-dlp spawn. Best-effort: callers treat an error
/// as "no thumbnail" and keep the placeholder.
pub async fn download_thumbnail(url: &str) -> Result<Vec<u8>> {
    let resp = oauth2::reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| WhsprError::Other(format!("thumbnail request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(WhsprError::Other(format!(
            "thumbnail request returned {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| WhsprError::Other(format!("reading thumbnail failed: {e}")))?;
    Ok(bytes.to_vec())
}

/// A per-call scratch directory under the system temp dir, named uniquely by
/// pid + a nanosecond clock read so concurrent imports never collide.
pub(crate) fn unique_temp_dir() -> Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("whspr-import-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_section_uses_star_time_range() {
        assert_eq!(ClipRange::new(4.5, 10.0).to_download_section(), "*4.5-10");
        assert_eq!(ClipRange::new(0.0, 90.0).to_download_section(), "*0-90");
    }

    #[test]
    fn clip_range_from_chapter_copies_bounds() {
        let chapter = Chapter {
            title: "Borrowing".to_string(),
            start_secs: 300.0,
            end_secs: 420.0,
        };
        let clip = ClipRange::from_chapter(&chapter);
        assert_eq!(clip.start_secs, 300.0);
        assert_eq!(clip.end_secs, 420.0);
        assert_eq!(clip.to_download_section(), "*300-420");
    }
}
