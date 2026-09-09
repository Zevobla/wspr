//! Resolving a URL to [`MediaInfo`] via `yt-dlp --dump-single-json`.
//!
//! This module holds the typed metadata a resolve produces. `MediaInfo` and
//! its parts derive `Serialize`/`Deserialize` so the app can cache a resolve
//! result without re-running yt-dlp.

use serde::{Deserialize, Serialize};

/// A caption/subtitle language advertised by yt-dlp, e.g. `en` ("English").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lang {
    /// The BCP-47-ish language code yt-dlp keys the track by (`en`, `es`, …).
    pub code: String,
    /// A human-readable name for the track, when yt-dlp provides one.
    pub name: Option<String>,
}

/// One chapter marker within a single media item. The app maps these to note
/// headings, and any one can be turned into a [`crate::ClipRange`] to import
/// just that section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start_secs: f32,
    pub end_secs: f32,
}

/// One item of a resolved playlist (a flat entry, not a full re-resolve).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub title: String,
    pub id: Option<String>,
    pub url: Option<String>,
    pub duration_secs: Option<f32>,
}

/// A resolved playlist: the app can queue the whole thing "as one course".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    /// yt-dlp's declared item count, falling back to `entries.len()`.
    pub count: usize,
    pub entries: Vec<PlaylistEntry>,
}

/// Everything a resolve learned about a URL without downloading any media.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub title: String,
    pub duration_secs: Option<f32>,
    pub uploader: Option<String>,
    pub chapters: Vec<Chapter>,
    /// Human-authored caption languages (`subtitles`).
    pub human_captions: Vec<Lang>,
    /// Machine-generated caption languages (`automatic_captions`).
    pub auto_captions: Vec<Lang>,
    /// Populated when the URL resolved to a playlist rather than one item.
    pub playlist: Option<Playlist>,
}

/// Where yt-dlp should read authentication cookies from, for private or
/// age-gated media. `None` runs anonymously.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CookiesFrom {
    #[default]
    None,
    /// Read cookies from an installed browser's profile, e.g. `"firefox"` or
    /// `"chrome"` — passed straight to `--cookies-from-browser`.
    Browser(String),
}
