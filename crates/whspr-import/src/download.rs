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
/// [`CookiesFrom::Browser`], authenticates via `--cookies-from-browser`. When
/// `progress` is set, yt-dlp's `[download] N%` line is streamed to it as a
/// `0..=100` percentage (best-effort). The caller owns and must delete the
/// returned file (see the module docs).
pub async fn download_audio(
    url: &str,
    clip: Option<ClipRange>,
    cookies: CookiesFrom,
    progress: Option<tokio::sync::mpsc::UnboundedSender<u8>>,
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
        // `bestaudio/best`: fall back to a combined stream when no audio-only
        // format is offered (some sites / older yt-dlp builds).
        .arg("bestaudio/best")
        .arg("--extract-audio")
        .arg("--audio-format")
        .arg("wav")
        // Force the extract-audio ffmpeg pass to emit 16kHz mono directly.
        .arg("--postprocessor-args")
        .arg("ExtractAudio:-ar 16000 -ac 1")
        // One progress line per update (no `\r`), so stdout can be streamed.
        .arg("--newline")
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
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| WhsprError::Other(format!("failed to run yt-dlp: {e}")))?;

    // Drain stderr concurrently so a chatty yt-dlp can't deadlock on a full
    // pipe while we read progress from stdout.
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut buf).await;
        }
        buf
    });

    if let Some(stdout) = child.stdout.take() {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let (Some(tx), Some(pct)) = (progress.as_ref(), parse_download_percent(&line)) {
                let _ = tx.send(pct);
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| WhsprError::Other(format!("failed to run yt-dlp: {e}")))?;
    let stderr = stderr_task.await.unwrap_or_default();
    if !status.success() {
        return Err(WhsprError::Other(format!(
            "yt-dlp audio download failed: {}",
            stderr.trim()
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

/// Parses a yt-dlp `--newline` progress line (`[download]  12.3% of …`) into a
/// `0..=100` percentage; `None` for any other line.
fn parse_download_percent(line: &str) -> Option<u8> {
    let rest = line.trim_start().strip_prefix("[download]")?;
    let pct: f32 = rest.trim_start().split('%').next()?.trim().parse().ok()?;
    Some(pct.clamp(0.0, 100.0) as u8)
}

/// [`download_audio`] plus decode/resample: returns the WAV path (for the
/// caller to delete) and the 16kHz mono [`AudioBuffer`] ready for whisper.
pub async fn download_to_audio(
    url: &str,
    clip: Option<ClipRange>,
    cookies: CookiesFrom,
    progress: Option<tokio::sync::mpsc::UnboundedSender<u8>>,
) -> Result<(PathBuf, AudioBuffer)> {
    let wav = download_audio(url, clip, cookies, progress).await?;
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
    fn parses_yt_dlp_download_percent() {
        assert_eq!(
            parse_download_percent("[download]  12.3% of ~16.00MiB at 1.20MiB/s ETA 00:10"),
            Some(12)
        );
        assert_eq!(
            parse_download_percent("[download] 100% of 16.00MiB"),
            Some(100)
        );
        assert_eq!(
            parse_download_percent("[download] Destination: audio.webm"),
            None
        );
        assert_eq!(parse_download_percent("[youtube] extracting url"), None);
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
