//! Resolving a URL to [`MediaInfo`] via `yt-dlp --dump-single-json`.
//!
//! [`parse_media_info`] is the pure, testable core that turns yt-dlp's JSON
//! into typed metadata. Parsing is done defensively off a
//! [`serde_json::Value`] rather than a rigid `#[derive(Deserialize)]` struct
//! because yt-dlp's schema is loose — `uploader`/`duration`/`chapters` are
//! routinely absent or `null`, and a playlist dump reshapes the top level
//! entirely — so every field is read best-effort and missing ones degrade to
//! empty/`None` instead of failing the whole parse.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use whspr_core::{Result, WhsprError};

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

/// Parses one `yt-dlp --dump-single-json` document into [`MediaInfo`].
///
/// Handles both a single media item and a `"_type": "playlist"` document.
/// Errors only if `json` isn't valid JSON at all; any individual missing
/// field degrades gracefully.
pub fn parse_media_info(json: &str) -> Result<MediaInfo> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| WhsprError::Other(format!("failed to parse yt-dlp JSON: {e}")))?;
    Ok(media_info_from_value(&value))
}

fn media_info_from_value(v: &Value) -> MediaInfo {
    let playlist = (v["_type"].as_str() == Some("playlist")).then(|| parse_playlist(v));
    MediaInfo {
        title: v["title"].as_str().unwrap_or_default().to_string(),
        duration_secs: v["duration"].as_f64().map(|d| d as f32),
        uploader: v["uploader"]
            .as_str()
            .or_else(|| v["channel"].as_str())
            .map(str::to_string),
        chapters: parse_chapters(&v["chapters"]),
        human_captions: parse_langs(&v["subtitles"]),
        auto_captions: parse_langs(&v["automatic_captions"]),
        playlist,
    }
}

/// Reads the `chapters` array. Entries without both a start and end time are
/// skipped (a chapter with no span can't be mapped to a clip).
fn parse_chapters(v: &Value) -> Vec<Chapter> {
    let Some(array) = v.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|c| {
            let start = c["start_time"].as_f64()? as f32;
            let end = c["end_time"].as_f64()? as f32;
            Some(Chapter {
                title: c["title"].as_str().unwrap_or_default().to_string(),
                start_secs: start,
                end_secs: end,
            })
        })
        .collect()
}

/// Reads a `subtitles` / `automatic_captions` object (lang code -> list of
/// format dicts) into a list of [`Lang`]. serde_json's default map is a
/// `BTreeMap`, so iteration order — and thus the output — is stable and
/// alphabetical by code.
fn parse_langs(v: &Value) -> Vec<Lang> {
    let Some(map) = v.as_object() else {
        return Vec::new();
    };
    map.iter()
        .map(|(code, formats)| Lang {
            code: code.clone(),
            name: formats
                .as_array()
                .and_then(|f| f.iter().find_map(|entry| entry["name"].as_str()))
                .map(str::to_string),
        })
        .collect()
}

fn parse_playlist(v: &Value) -> Playlist {
    let entries: Vec<PlaylistEntry> = v["entries"]
        .as_array()
        .map(|array| array.iter().map(playlist_entry_from_value).collect())
        .unwrap_or_default();
    let count = v["playlist_count"]
        .as_u64()
        .map(|c| c as usize)
        .unwrap_or(entries.len());
    Playlist { count, entries }
}

fn playlist_entry_from_value(v: &Value) -> PlaylistEntry {
    PlaylistEntry {
        title: v["title"].as_str().unwrap_or_default().to_string(),
        id: v["id"].as_str().map(str::to_string),
        url: v["webpage_url"]
            .as_str()
            .or_else(|| v["url"].as_str())
            .map(str::to_string),
        duration_secs: v["duration"].as_f64().map(|d| d as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
        std::fs::read_to_string(format!("{path}{name}"))
            .unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
    }

    #[test]
    fn parses_single_video_metadata() {
        let info = parse_media_info(&fixture("single_video.json")).expect("parse");

        assert_eq!(info.title, "Intro to Rust Ownership");
        assert_eq!(info.duration_secs, Some(754.0));
        assert_eq!(info.uploader.as_deref(), Some("Rust Academy"));
        assert!(info.playlist.is_none());
    }

    #[test]
    fn parses_chapters_in_order() {
        let info = parse_media_info(&fixture("single_video.json")).expect("parse");

        assert_eq!(info.chapters.len(), 3);
        assert_eq!(info.chapters[0].title, "Introduction");
        assert_eq!(info.chapters[0].start_secs, 0.0);
        assert_eq!(info.chapters[0].end_secs, 90.0);
        assert_eq!(info.chapters[2].title, "Borrowing");
        assert_eq!(info.chapters[2].start_secs, 300.0);
    }

    #[test]
    fn separates_human_and_auto_caption_langs() {
        let info = parse_media_info(&fixture("single_video.json")).expect("parse");

        // BTreeMap-backed => alphabetical, deterministic.
        let human: Vec<&str> = info
            .human_captions
            .iter()
            .map(|l| l.code.as_str())
            .collect();
        assert_eq!(human, ["en", "es"]);
        assert_eq!(info.human_captions[0].name.as_deref(), Some("English"));

        let auto: Vec<&str> = info.auto_captions.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(auto, ["de", "en", "fr"]);
    }

    #[test]
    fn parses_playlist_entries_and_count() {
        let info = parse_media_info(&fixture("playlist.json")).expect("parse");

        let playlist = info.playlist.expect("should be a playlist");
        assert_eq!(playlist.count, 3);
        assert_eq!(playlist.entries.len(), 3);
        assert_eq!(playlist.entries[0].title, "Lecture 1 — Variables");
        assert_eq!(playlist.entries[0].id.as_deref(), Some("aaa111"));
        assert_eq!(playlist.entries[1].duration_secs, Some(612.0));
    }

    #[test]
    fn missing_fields_degrade_gracefully() {
        let info = parse_media_info(r#"{"title":"bare"}"#).expect("parse");
        assert_eq!(info.title, "bare");
        assert_eq!(info.duration_secs, None);
        assert_eq!(info.uploader, None);
        assert!(info.chapters.is_empty());
        assert!(info.human_captions.is_empty());
        assert!(info.playlist.is_none());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_media_info("not json at all").is_err());
    }
}
