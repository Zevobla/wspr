//! Downloading a URL's audio, and the time-window selection that drives it.
//!
//! [`ClipRange`] is a half-open second range — built from a user selection or
//! straight from a [`Chapter`] — that renders to a `yt-dlp
//! --download-sections` value so only part of a long recording is fetched.

use crate::resolve::Chapter;

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
