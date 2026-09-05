//! Timecoded export for `whspr transcribe --format srt|vtt`: renders a
//! `Transcript`'s segments as SubRip (SRT) or WebVTT cues. Split out of
//! `main.rs` to keep that file under this project's 600-line-per-file
//! guideline.

use std::str::FromStr;

use whspr_core::Transcript;

/// Timecoded export format for `transcribe`'s `--format` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Srt,
    Vtt,
}

impl FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "srt" => Ok(ExportFormat::Srt),
            "vtt" | "webvtt" => Ok(ExportFormat::Vtt),
            _ => Err(format!(
                "unknown export format: {s} (expected \"srt\" or \"vtt\")"
            )),
        }
    }
}

/// One timed cue, borrowed from either a real `TranscriptSegment` or the
/// degenerate single-cue fallback built by `cues` below.
struct Cue<'a> {
    start_secs: f32,
    end_secs: f32,
    text: &'a str,
}

/// The cues to render for `transcript`. If the backend populated
/// `segments` (real `WhisperLocal` output), those are used directly.
/// Otherwise (e.g. `MockAsr`'s canned `Transcript`, which never sets
/// `segments`) this falls back to a single cue spanning
/// `fallback_duration_secs` so a timecoded export never panics or
/// silently drops the text - it just degrades to one untimed-looking cue.
/// An entirely empty transcript (no segments, no text) yields no cues at
/// all.
fn cues(transcript: &Transcript, fallback_duration_secs: f32) -> Vec<Cue<'_>> {
    if !transcript.segments.is_empty() {
        return transcript
            .segments
            .iter()
            .map(|s| Cue {
                start_secs: s.start_secs,
                end_secs: s.end_secs,
                text: &s.text,
            })
            .collect();
    }
    if transcript.text.trim().is_empty() {
        return Vec::new();
    }
    vec![Cue {
        start_secs: 0.0,
        end_secs: fallback_duration_secs.max(0.0),
        text: &transcript.text,
    }]
}

/// Formats a non-negative second count as `HH:MM:SS<sep>mmm`. `sep` is `,`
/// for SRT and `.` for VTT - the only difference between the two formats'
/// timestamp syntax.
fn format_timestamp(secs: f32, sep: char) -> String {
    let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{h:02}:{m:02}:{s:02}{sep}{ms:03}")
}

/// Renders every cue as `<index>\n<start> --> <end>\n<text>\n\n`, joined
/// and trimmed of the trailing blank line - shared between `to_srt` and
/// `to_vtt`, which differ only in the timestamp separator and (for VTT) a
/// leading header.
fn render_cues(cues: &[Cue<'_>], sep: char) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_timestamp(cue.start_secs, sep),
            format_timestamp(cue.end_secs, sep),
            cue.text.trim(),
        ));
    }
    out.trim_end().to_string()
}

/// Renders `transcript` as SubRip (`.srt`) cues.
pub fn to_srt(transcript: &Transcript, fallback_duration_secs: f32) -> String {
    render_cues(&cues(transcript, fallback_duration_secs), ',')
}

/// Renders `transcript` as WebVTT (`.vtt`) cues, including the required
/// `WEBVTT` header line.
pub fn to_vtt(transcript: &Transcript, fallback_duration_secs: f32) -> String {
    let body = render_cues(&cues(transcript, fallback_duration_secs), '.');
    if body.is_empty() {
        "WEBVTT".to_string()
    } else {
        format!("WEBVTT\n\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whspr_core::TranscriptSegment;

    fn transcript_with_segments() -> Transcript {
        Transcript {
            text: "hello world goodbye".to_string(),
            language: None,
            segments: vec![
                TranscriptSegment {
                    text: "hello world".to_string(),
                    start_secs: 0.0,
                    end_secs: 2.5,
                    speaker: None,
                },
                TranscriptSegment {
                    text: "goodbye".to_string(),
                    start_secs: 2.5,
                    end_secs: 5.0,
                    speaker: None,
                },
            ],
        }
    }

    #[test]
    fn export_format_from_str_accepts_known_spellings() {
        assert_eq!(ExportFormat::from_str("srt"), Ok(ExportFormat::Srt));
        assert_eq!(ExportFormat::from_str("SRT"), Ok(ExportFormat::Srt));
        assert_eq!(ExportFormat::from_str("vtt"), Ok(ExportFormat::Vtt));
        assert_eq!(ExportFormat::from_str("webvtt"), Ok(ExportFormat::Vtt));
        assert!(ExportFormat::from_str("txt").is_err());
    }

    #[test]
    fn format_timestamp_pads_hours_minutes_seconds_millis() {
        assert_eq!(format_timestamp(0.0, ','), "00:00:00,000");
        assert_eq!(format_timestamp(2.5, ','), "00:00:02,500");
        assert_eq!(format_timestamp(3661.25, '.'), "01:01:01.250");
    }

    #[test]
    fn to_srt_renders_one_cue_per_segment() {
        let srt = to_srt(&transcript_with_segments(), 0.0);
        assert_eq!(
            srt,
            "1\n00:00:00,000 --> 00:00:02,500\nhello world\n\n\
             2\n00:00:02,500 --> 00:00:05,000\ngoodbye"
        );
    }

    #[test]
    fn to_vtt_has_webvtt_header_and_dot_separated_millis() {
        let vtt = to_vtt(&transcript_with_segments(), 0.0);
        assert!(vtt.starts_with("WEBVTT\n\n1\n"));
        assert!(vtt.contains("00:00:00.000 --> 00:00:02.500"));
    }

    #[test]
    fn to_srt_falls_back_to_single_cue_when_no_segments() {
        // MockAsr's canned Transcript never populates `segments` - this is
        // the degenerate path that must never crash.
        let transcript = Transcript {
            text: "the quick brown fox".to_string(),
            ..Default::default()
        };
        let srt = to_srt(&transcript, 3.2);
        assert_eq!(srt, "1\n00:00:00,000 --> 00:00:03,200\nthe quick brown fox");
    }

    #[test]
    fn to_srt_of_empty_transcript_is_empty() {
        assert_eq!(to_srt(&Transcript::default(), 0.0), "");
    }

    #[test]
    fn to_vtt_of_empty_transcript_is_just_the_header() {
        assert_eq!(to_vtt(&Transcript::default(), 0.0), "WEBVTT");
    }
}
