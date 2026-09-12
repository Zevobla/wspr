//! Resolving a URL to [`MediaInfo`] via `yt-dlp --dump-single-json`.
//!
//! [`resolve`] runs the tool; [`parse_media_info`] is the pure, testable core
//! that turns its JSON into typed metadata. Parsing is done defensively off a
//! [`serde_json::Value`] rather than a rigid `#[derive(Deserialize)]` struct
//! because yt-dlp's schema is loose — `uploader`/`duration`/`chapters` are
//! routinely absent or `null`, and a playlist dump reshapes the top level
//! entirely — so every field is read best-effort and missing ones degrade to
//! empty/`None` instead of failing the whole parse.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use whspr_core::{Result, WhsprError};

use crate::tools::{resolve_tool, Tool};

/// A caption/subtitle language advertised by yt-dlp, e.g. `en` ("English").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lang {
    /// The BCP-47-ish language code yt-dlp keys the track by (`en`, `es`, …).
    pub code: String,
    /// A human-readable name for the track, when yt-dlp provides one.
    pub name: Option<String>,
    /// Direct download URL for this track's best format (see
    /// [`pick_caption_format`]), lifted straight from yt-dlp's dump. Fetching
    /// it over plain HTTP sidesteps a second yt-dlp spawn — which would re-hit
    /// YouTube's frequently bot-walled player API — so captions import even
    /// when the stream formats themselves are gated.
    pub url: Option<String>,
    /// The extension matching [`url`](Self::url) (`json3`/`vtt`/`srv1`…), so
    /// the fetched bytes can be routed to the right parser.
    pub ext: Option<String>,
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

/// Everything [`resolve`] learned about a URL without downloading any media.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub title: String,
    pub duration_secs: Option<f32>,
    pub uploader: Option<String>,
    /// yt-dlp's `upload_date`, a `"YYYYMMDD"` string when present.
    pub upload_date: Option<String>,
    /// yt-dlp's `thumbnail` URL, when present.
    pub thumbnail: Option<String>,
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

/// Resolves `url` to [`MediaInfo`] by running `yt-dlp --dump-single-json`.
///
/// With [`CookiesFrom::Browser`], adds `--cookies-from-browser <name>` so
/// private/age-gated media resolves. Errors if yt-dlp can't be found (see
/// [`resolve_tool`]), can't be spawned, or exits non-zero.
pub async fn resolve(url: &str, cookies: CookiesFrom) -> Result<MediaInfo> {
    let ytdlp = resolve_tool(Tool::YtDlp).ok_or_else(|| {
        WhsprError::Other(
            "yt-dlp not found: install it or point WHSPR_YTDLP at the binary".to_string(),
        )
    })?;

    let mut cmd = tokio::process::Command::new(&ytdlp);
    cmd.arg("--dump-single-json").arg("--no-warnings");
    // Resolve is metadata-only: title, chapters, caption tracks — never a
    // download. When YouTube bot-walls a video's stream formats (the
    // intermittent "The page needs to be reloaded" / "Requested format is not
    // available" response), yt-dlp still extracts all the page metadata but
    // then aborts non-zero because format selection found nothing. This flag
    // tells it not to treat "no downloadable format" as fatal, so a probe that
    // only wants metadata still succeeds. It is NOT a player-client hack: it
    // changes nothing about which formats exist, only whether their absence
    // fails a metadata dump.
    cmd.arg("--ignore-no-formats-error");
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
            "yt-dlp --dump-single-json failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    parse_media_info(&String::from_utf8_lossy(&output.stdout))
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
        upload_date: v["upload_date"].as_str().map(str::to_string),
        thumbnail: pick_thumbnail(v),
        chapters: parse_chapters(&v["chapters"]),
        human_captions: parse_langs(&v["subtitles"]),
        auto_captions: parse_auto_captions(&v["automatic_captions"]),
        playlist,
    }
}

/// Picks a small/medium thumbnail URL from yt-dlp's `thumbnails` array -- the
/// widest that's still `<= 640px` (a quick-to-fetch preview, not the ~1280px
/// maxres), else the narrowest available, else the top-level `thumbnail`.
fn pick_thumbnail(v: &Value) -> Option<String> {
    if let Some(arr) = v["thumbnails"].as_array() {
        let sized: Vec<(&str, i64)> = arr
            .iter()
            .filter_map(|t| Some((t["url"].as_str()?, t["width"].as_i64()?)))
            .collect();
        let pick = sized
            .iter()
            .filter(|(_, w)| *w <= 640)
            .max_by_key(|(_, w)| *w)
            .or_else(|| sized.iter().min_by_key(|(_, w)| *w));
        if let Some((url, _)) = pick {
            return Some(url.to_string());
        }
    }
    v["thumbnail"].as_str().map(str::to_string)
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
        .map(|(code, formats)| {
            let formats = formats.as_array();
            let (url, ext) = formats.map(|f| pick_caption_format(f)).unwrap_or((None, None));
            Lang {
                code: code.clone(),
                name: formats
                    .and_then(|f| f.iter().find_map(|entry| entry["name"].as_str()))
                    .map(str::to_string),
                url,
                ext,
            }
        })
        .collect()
}

/// Picks the best downloadable format from one track's format list, preferring
/// `json3` (richest timing) then `vtt` then `srv1`. Only these three are
/// considered because they're exactly what the caption parser handles — never
/// return an `ext` (`ttml`, `srv3`, …) the fetch path can't parse, so a track
/// without a usable format degrades to `(None, None)` instead of a silently
/// empty transcript. Returns the chosen `(url, ext)`.
fn pick_caption_format(formats: &[Value]) -> (Option<String>, Option<String>) {
    const PREF: [&str; 3] = ["json3", "vtt", "srv1"];
    let chosen = PREF.iter().find_map(|want| {
        formats
            .iter()
            .find(|f| f["ext"].as_str() == Some(want) && f["url"].as_str().is_some())
    });
    match chosen {
        Some(f) => (
            f["url"].as_str().map(str::to_string),
            f["ext"].as_str().map(str::to_string),
        ),
        None => (None, None),
    }
}

/// Reads `automatic_captions`, but collapses YouTube's translation spam.
///
/// yt-dlp lists the source ASR track keyed `<lang>-orig` (e.g. `ru-orig`)
/// alongside ~150 machine *translations* of it into every other language. Only
/// the source is a genuine transcript of the audio; the translations are
/// derived noise that would flood the tag row and make an alphabetical
/// `.first()` pick "Afar" as the default. So when any `-orig` track is present,
/// keep just those; otherwise (older dumps / non-YouTube sites with no `-orig`
/// marker) keep the list untouched.
fn parse_auto_captions(v: &Value) -> Vec<Lang> {
    let langs = parse_langs(v);
    if langs.iter().any(|l| l.code.ends_with("-orig")) {
        langs
            .into_iter()
            .filter(|l| l.code.ends_with("-orig"))
            .collect()
    } else {
        langs
    }
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

    // Fixtures are embedded as string constants (not read from
    // tests/fixtures/*.json) so they survive the crane hermetic-build source
    // filter, which keeps only Rust/Cargo sources (+ .svg) and would strip
    // external .json files (see flake.nix). This mirrors the exact shape of a
    // real `yt-dlp --dump-single-json` document.
    const SINGLE_VIDEO_JSON: &str = r#"{
      "_type": "video",
      "id": "abc123",
      "title": "Intro to Rust Ownership",
      "duration": 754,
      "uploader": "Rust Academy",
      "upload_date": "20230115",
      "thumbnail": "https://example.com/thumb.jpg",
      "channel": "Rust Academy",
      "webpage_url": "https://example.com/watch?v=abc123",
      "chapters": [
        { "start_time": 0, "end_time": 90, "title": "Introduction" },
        { "start_time": 90, "end_time": 300, "title": "Move Semantics" },
        { "start_time": 300, "end_time": 754, "title": "Borrowing" }
      ],
      "subtitles": {
        "en": [{ "ext": "vtt", "url": "https://example.com/en.vtt", "name": "English" }],
        "es": [{ "ext": "vtt", "url": "https://example.com/es.vtt", "name": "Spanish" }]
      },
      "automatic_captions": {
        "en": [{ "ext": "json3", "name": "English (auto-generated)" }],
        "de": [{ "ext": "json3", "name": "German (auto-generated)" }],
        "fr": [{ "ext": "json3", "name": "French (auto-generated)" }]
      }
    }"#;

    const PLAYLIST_JSON: &str = r#"{
      "_type": "playlist",
      "id": "PL123",
      "title": "Rust Course",
      "playlist_count": 3,
      "entries": [
        { "title": "Lecture 1 — Variables", "id": "aaa111",
          "webpage_url": "https://example.com/watch?v=aaa111", "duration": 540 },
        { "title": "Lecture 2 — Functions", "id": "bbb222",
          "webpage_url": "https://example.com/watch?v=bbb222", "duration": 612 },
        { "title": "Lecture 3 — Ownership", "id": "ccc333",
          "webpage_url": "https://example.com/watch?v=ccc333", "duration": 700 }
      ]
    }"#;

    #[test]
    fn parses_single_video_metadata() {
        let info = parse_media_info(SINGLE_VIDEO_JSON).expect("parse");

        assert_eq!(info.title, "Intro to Rust Ownership");
        assert_eq!(info.duration_secs, Some(754.0));
        assert_eq!(info.uploader.as_deref(), Some("Rust Academy"));
        assert_eq!(info.upload_date.as_deref(), Some("20230115"));
        assert_eq!(
            info.thumbnail.as_deref(),
            Some("https://example.com/thumb.jpg")
        );
        assert!(info.playlist.is_none());
    }

    #[test]
    fn parses_chapters_in_order() {
        let info = parse_media_info(SINGLE_VIDEO_JSON).expect("parse");

        assert_eq!(info.chapters.len(), 3);
        assert_eq!(info.chapters[0].title, "Introduction");
        assert_eq!(info.chapters[0].start_secs, 0.0);
        assert_eq!(info.chapters[0].end_secs, 90.0);
        assert_eq!(info.chapters[2].title, "Borrowing");
        assert_eq!(info.chapters[2].start_secs, 300.0);
    }

    #[test]
    fn separates_human_and_auto_caption_langs() {
        let info = parse_media_info(SINGLE_VIDEO_JSON).expect("parse");

        // BTreeMap-backed => alphabetical, deterministic.
        let human: Vec<&str> = info
            .human_captions
            .iter()
            .map(|l| l.code.as_str())
            .collect();
        assert_eq!(human, ["en", "es"]);
        assert_eq!(info.human_captions[0].name.as_deref(), Some("English"));
        // The track's downloadable format URL + ext are carried through so the
        // import can fetch it directly instead of re-spawning yt-dlp.
        assert_eq!(
            info.human_captions[0].url.as_deref(),
            Some("https://example.com/en.vtt")
        );
        assert_eq!(info.human_captions[0].ext.as_deref(), Some("vtt"));

        let auto: Vec<&str> = info.auto_captions.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(auto, ["de", "en", "fr"]);
    }

    #[test]
    fn caption_format_prefers_json3_then_vtt() {
        let formats = serde_json::json!([
            { "ext": "srv1", "url": "https://example.com/a.srv1" },
            { "ext": "vtt", "url": "https://example.com/a.vtt" },
            { "ext": "json3", "url": "https://example.com/a.json3" }
        ]);
        let (url, ext) = pick_caption_format(formats.as_array().unwrap());
        assert_eq!(url.as_deref(), Some("https://example.com/a.json3"));
        assert_eq!(ext.as_deref(), Some("json3"));

        // No json3 => falls back to vtt over the srv variants.
        let formats = serde_json::json!([
            { "ext": "srv3", "url": "https://example.com/a.srv3" },
            { "ext": "vtt", "url": "https://example.com/a.vtt" }
        ]);
        let (_, ext) = pick_caption_format(formats.as_array().unwrap());
        assert_eq!(ext.as_deref(), Some("vtt"));
    }

    #[test]
    fn auto_captions_keep_only_the_source_orig_track() {
        // Real YouTube shape: the source ASR track (`ru-orig`) plus its machine
        // translations into every language. Only the source survives.
        let json = r#"{
          "title": "видео",
          "automatic_captions": {
            "aa": [{ "ext": "json3", "url": "https://x/aa.json3" }],
            "en": [{ "ext": "json3", "url": "https://x/en.json3" }],
            "ru": [{ "ext": "json3", "url": "https://x/ru.json3" }],
            "ru-orig": [{ "ext": "json3", "url": "https://x/ru-orig.json3" }]
          }
        }"#;
        let info = parse_media_info(json).expect("parse");
        let codes: Vec<&str> = info.auto_captions.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["ru-orig"]);
        assert_eq!(
            info.auto_captions[0].url.as_deref(),
            Some("https://x/ru-orig.json3")
        );
    }

    #[test]
    fn parses_playlist_entries_and_count() {
        let info = parse_media_info(PLAYLIST_JSON).expect("parse");

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
        assert_eq!(info.upload_date, None);
        assert_eq!(info.thumbnail, None);
        assert!(info.chapters.is_empty());
        assert!(info.human_captions.is_empty());
        assert!(info.playlist.is_none());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_media_info("not json at all").is_err());
    }

    /// Real end-to-end resolve against a public URL with the actual `yt-dlp`
    /// binary. `#[ignore]`d — it needs network and yt-dlp installed, so it
    /// never runs in the offline gate (mirroring whspr-asr's real-model
    /// test). Run it explicitly:
    ///
    /// ```sh
    /// cargo test -p whspr-import -- --ignored
    /// ```
    #[tokio::test]
    #[ignore]
    async fn resolves_a_real_public_url() {
        if resolve_tool(Tool::YtDlp).is_none() {
            eprintln!("skipping resolves_a_real_public_url: yt-dlp not found (set WHSPR_YTDLP)");
            return;
        }
        // A short, stable, Creative-Commons public clip.
        let url = "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
        let info = resolve(url, CookiesFrom::None)
            .await
            .expect("resolve should succeed against a public URL");
        assert!(!info.title.is_empty(), "resolved media should have a title");
        assert!(
            info.duration_secs.is_some(),
            "resolved media should report a duration"
        );
    }
}
