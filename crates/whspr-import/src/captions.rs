//! Turning an already-downloaded caption track into a
//! [`whspr_core::Transcript`] — with per-segment timestamps and **no model**.
//!
//! Three formats yt-dlp can hand back are supported, selected by
//! [`CaptionFormat`]: WebVTT (`.vtt`), the srv1-style XML transcript, and the
//! modern `json3`. Each becomes one [`whspr_core::TranscriptSegment`] per
//! timed cue. Parsing is infallible: malformed or empty input yields a
//! transcript with no segments rather than an error, so a bad caption file
//! just falls back to the audio path instead of aborting an import.

use serde_json::Value;

use whspr_core::{Transcript, TranscriptSegment};

/// Which caption serialization [`parse_captions`] is being handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionFormat {
    /// WebVTT (`WEBVTT`-headed `.vtt`), the common `--sub-format vtt` output.
    WebVtt,
    /// yt-dlp's srv1-style XML: `<text start=".." dur="..">…</text>`.
    Srv,
    /// yt-dlp's `json3`: `{"events":[{"tStartMs":..,"segs":[{"utf8":..}]}]}`.
    Json3,
}

/// Parses a caption track into a [`Transcript`]. The transcript's `text` is
/// every segment joined by a single space; `language` is left `None` (the
/// caller already knows which language track it fetched).
pub fn parse_captions(vtt_or_srv: &str, format: CaptionFormat) -> Transcript {
    let segments = match format {
        CaptionFormat::WebVtt => parse_vtt(vtt_or_srv),
        CaptionFormat::Srv => parse_srv(vtt_or_srv),
        CaptionFormat::Json3 => parse_json3(vtt_or_srv),
    };
    let text = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Transcript {
        text,
        language: None,
        segments,
    }
}

/// Collapses internal runs of whitespace to single spaces and trims — cue
/// text routinely spans several physical lines that read as one utterance.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses `HH:MM:SS.mmm` / `MM:SS.mmm` (colon-separated, most-significant
/// first) into seconds.
fn parse_vtt_timestamp(s: &str) -> Option<f32> {
    let mut secs = 0f32;
    for part in s.trim().split(':') {
        secs = secs * 60.0 + part.parse::<f32>().ok()?;
    }
    Some(secs)
}

/// Removes WebVTT inline markup tags (`<c>`, `<00:00:01.000>`, `</c>`, …).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn parse_vtt(text: &str) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((left, right)) = line.split_once("-->") else {
            continue; // header, cue id, blank, NOTE/STYLE — not a timing line
        };
        let Some(start) = parse_vtt_timestamp(left) else {
            continue;
        };
        let Some(end) = right
            .split_whitespace()
            .next()
            .and_then(parse_vtt_timestamp)
        else {
            continue;
        };

        // Cue payload: every line up to the next blank line.
        let mut payload = String::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                break;
            }
            if !payload.is_empty() {
                payload.push(' ');
            }
            payload.push_str(&strip_tags(next));
            lines.next();
        }

        let payload = normalize_ws(&payload);
        if !payload.is_empty() {
            segments.push(TranscriptSegment {
                text: payload,
                start_secs: start,
                end_secs: end,
                speaker: None,
            });
        }
    }
    segments
}

/// Unescapes the handful of XML entities srv tracks use.
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Reads a double-quoted attribute value out of an opening-tag body, e.g.
/// `start="4.5" dur="3"` -> `attr_value(body, "start") == Some("4.5")`.
fn attr_value<'a>(tag_body: &'a str, attr: &str) -> Option<&'a str> {
    let key = format!("{attr}=\"");
    let start = tag_body.find(&key)? + key.len();
    let rest = &tag_body[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_srv(text: &str) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    // Walk each `<text ...>…</text>` element without a full XML parser.
    let mut rest = text;
    while let Some(open_start) = rest.find("<text") {
        rest = &rest[open_start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let tag_body = &rest[..open_end];
        let after_open = &rest[open_end + 1..];
        let Some(close) = after_open.find("</text>") else {
            break;
        };
        let inner = &after_open[..close];
        rest = &after_open[close + "</text>".len()..];

        let Some(start) = attr_value(tag_body, "start").and_then(|s| s.parse::<f32>().ok()) else {
            continue;
        };
        let dur = attr_value(tag_body, "dur")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        let payload = normalize_ws(&xml_unescape(inner));
        if !payload.is_empty() {
            segments.push(TranscriptSegment {
                text: payload,
                start_secs: start,
                end_secs: start + dur,
                speaker: None,
            });
        }
    }
    segments
}

fn parse_json3(text: &str) -> Vec<TranscriptSegment> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(events) = value["events"].as_array() else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    for event in events {
        // Window/style-only events carry no `segs`; skip them.
        let Some(segs) = event["segs"].as_array() else {
            continue;
        };
        let joined: String = segs.iter().filter_map(|s| s["utf8"].as_str()).collect();
        let payload = normalize_ws(&joined);
        if payload.is_empty() {
            continue;
        }
        let start_ms = event["tStartMs"].as_f64().unwrap_or(0.0);
        let dur_ms = event["dDurationMs"].as_f64().unwrap_or(0.0);
        segments.push(TranscriptSegment {
            text: payload,
            start_secs: (start_ms / 1000.0) as f32,
            end_secs: ((start_ms + dur_ms) / 1000.0) as f32,
            speaker: None,
        });
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    // Embedded as string constants (not read from tests/fixtures/*) so they
    // survive crane's hermetic-build source filter, which strips non-Rust
    // files (see flake.nix). Each mirrors a real yt-dlp caption track.
    const CAPTIONS_VTT: &str = "WEBVTT\n\n\
        1\n\
        00:00:01.000 --> 00:00:04.000\n\
        Welcome to the lecture.\n\n\
        2\n\
        00:00:04.500 --> 00:00:08.000\n\
        Today we talk\n\
        about <c>ownership</c>.\n\n\
        3\n\
        00:01:04.500 --> 00:01:08.000\n\
        Third cue here.\n";

    const CAPTIONS_SRV: &str = concat!(
        r#"<?xml version="1.0" encoding="utf-8"?><transcript>"#,
        r#"<text start="0" dur="4.5">Hello there</text>"#,
        r#"<text start="4.5" dur="3">general listener</text>"#,
        r#"<text start="7.5" dur="2.5">Rust &amp; friends</text>"#,
        r#"</transcript>"#,
    );

    const CAPTIONS_JSON3: &str = r#"{
      "events": [
        { "tStartMs": 0, "dDurationMs": 2500, "segs": [{ "utf8": "Hello" }, { "utf8": " world" }] },
        { "tStartMs": 2500, "dDurationMs": 1500, "segs": [{ "utf8": "second cue" }] },
        { "tStartMs": 5000, "dDurationMs": 100, "segs": [{ "utf8": "\n" }] },
        { "tStartMs": 9999, "wWinId": 1 }
      ]
    }"#;

    #[test]
    fn parses_webvtt_segments() {
        let t = parse_captions(CAPTIONS_VTT, CaptionFormat::WebVtt);

        assert_eq!(t.segments.len(), 3);
        assert_eq!(t.segments[0].text, "Welcome to the lecture.");
        assert_eq!(t.segments[0].start_secs, 1.0);
        assert_eq!(t.segments[0].end_secs, 4.0);
        // Second cue spans two physical lines joined into one, tags stripped.
        assert_eq!(t.segments[1].text, "Today we talk about ownership.");
        assert_eq!(t.segments[1].start_secs, 4.5);
        assert_eq!(t.segments[2].start_secs, 64.5); // crosses a minute boundary
        assert!(t.text.starts_with("Welcome to the lecture."));
    }

    #[test]
    fn parses_srv_xml_segments() {
        let t = parse_captions(CAPTIONS_SRV, CaptionFormat::Srv);

        assert_eq!(t.segments.len(), 3);
        assert_eq!(t.segments[0].text, "Hello there");
        assert_eq!(t.segments[0].start_secs, 0.0);
        assert_eq!(t.segments[0].end_secs, 4.5);
        // Entity unescaping: `Rust &amp; friends` -> `Rust & friends`.
        assert_eq!(t.segments[2].text, "Rust & friends");
    }

    #[test]
    fn parses_json3_segments_skipping_windowed_events() {
        let t = parse_captions(CAPTIONS_JSON3, CaptionFormat::Json3);

        assert_eq!(t.segments.len(), 2);
        assert_eq!(t.segments[0].text, "Hello world");
        assert_eq!(t.segments[0].start_secs, 0.0);
        assert_eq!(t.segments[0].end_secs, 2.5);
        assert_eq!(t.segments[1].text, "second cue");
        assert_eq!(t.segments[1].start_secs, 2.5);
    }

    #[test]
    fn malformed_input_yields_no_segments() {
        assert!(parse_captions("not vtt", CaptionFormat::WebVtt)
            .segments
            .is_empty());
        assert!(parse_captions("<broken", CaptionFormat::Srv)
            .segments
            .is_empty());
        assert!(parse_captions("{not json", CaptionFormat::Json3)
            .segments
            .is_empty());
    }
}
