//! Rolling-window transcription orchestration.
//!
//! Long audio (an imported file, or — in a later phase — a live longform
//! capture) is more than any single ASR call wants to swallow at once. This
//! module transcribes it in overlapping windows and folds each window's
//! segments back into one running, absolutely-timestamped transcript.
//!
//! It sits *above* the [`AsrBackend`](crate::AsrBackend) trait: it slices an
//! [`AudioBuffer`](crate::AudioBuffer) into sub-buffers, hands each to a
//! backend, and stitches the results. The trait itself is unchanged, and the
//! core logic here is pure — [`stitch`] is deterministic and synchronous, and
//! [`RollingTranscriber`] only reaches for `async` to call the backend.
//!
//! Windows deliberately overlap by a few seconds so the ASR backend keeps a
//! little context across a boundary and doesn't clip a word mid-sound. That
//! overlap means the last segment(s) of one window and the first of the next
//! describe the *same* audio, so [`stitch`] drops the duplicate "seam"
//! (matched by overlapping time range plus equal/near-equal text) rather than
//! emitting the words twice.

use crate::types::TranscriptSegment;

/// Default window length, in seconds — how much audio each ASR call sees.
pub const DEFAULT_WINDOW_SECS: f32 = 20.0;

/// Default overlap between consecutive windows, in seconds — shared audio
/// kept so the backend has context across a boundary (and the source of the
/// duplicate "seam" [`stitch`] dedupes).
pub const DEFAULT_OVERLAP_SECS: f32 = 3.0;

/// Offsets every `window` segment by `window_start_secs`, appends the ones
/// that aren't already present at the overlap seam to `existing`, and returns
/// the combined, time-monotonic timeline.
///
/// Pure and deterministic — no async, no I/O. `window` segments carry times
/// *relative to the start of their window*; each is shifted into absolute
/// time by adding `window_start_secs`. A shifted segment is dropped as a seam
/// duplicate when it overlaps in time with an `existing` segment **and** their
/// texts match once normalized (case/punctuation/whitespace folded away), so
/// words shared across an overlapping boundary appear once, not twice. The
/// result's segment starts are guaranteed non-decreasing.
pub fn stitch(
    existing: &[TranscriptSegment],
    window: &[TranscriptSegment],
    window_start_secs: f32,
) -> Vec<TranscriptSegment> {
    let mut out: Vec<TranscriptSegment> = existing.to_vec();

    for seg in window {
        let mut shifted = seg.clone();
        shifted.start_secs += window_start_secs;
        shifted.end_secs += window_start_secs;

        if is_seam_duplicate(existing, &shifted) {
            continue;
        }
        out.push(shifted);
    }

    enforce_monotonic_starts(&mut out);
    out
}

/// True when `candidate` re-states an `existing` segment across the overlap
/// seam: it overlaps one in time *and* carries the same normalized text.
fn is_seam_duplicate(existing: &[TranscriptSegment], candidate: &TranscriptSegment) -> bool {
    let candidate_text = normalized(&candidate.text);
    existing.iter().any(|prev| {
        time_ranges_overlap(prev, candidate) && normalized(&prev.text) == candidate_text
    })
}

/// Half-open time-range overlap: `true` unless the two segments are disjoint
/// or merely touch at an endpoint (adjacent segments do not overlap).
fn time_ranges_overlap(a: &TranscriptSegment, b: &TranscriptSegment) -> bool {
    a.start_secs < b.end_secs && b.start_secs < a.end_secs
}

/// Folds a segment's text to its comparable core: lowercased, split on any
/// non-alphanumeric character, re-joined with single spaces. Makes the seam
/// dedupe robust to the punctuation/casing an ASR backend sprinkles
/// differently on either side of an overlap (`"Hello,"` vs `"hello"`).
fn normalized(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Clamps each segment's start up to its predecessor's so the timeline never
/// steps backwards, keeping `end >= start` for any segment it nudges.
fn enforce_monotonic_starts(segments: &mut [TranscriptSegment]) {
    for i in 1..segments.len() {
        let floor = segments[i - 1].start_secs;
        if segments[i].start_secs < floor {
            segments[i].start_secs = floor;
        }
        if segments[i].end_secs < segments[i].start_secs {
            segments[i].end_secs = segments[i].start_secs;
        }
    }
}

/// Transcribes a long [`AudioBuffer`] as a sequence of overlapping windows,
/// accumulating one stitched, absolutely-timestamped transcript.
///
/// The caller drives it one window at a time via [`push_window`], supplying
/// each window's absolute start; the transcriber slices the matching samples,
/// runs the backend, and folds the result in with [`stitch`]. Stepping by
/// [`step_secs`] (window minus overlap) walks a buffer end to end.
///
/// [`push_window`]: RollingTranscriber::push_window
/// [`step_secs`]: RollingTranscriber::step_secs
pub struct RollingTranscriber {
    window_secs: f32,
    overlap_secs: f32,
    accumulated: Vec<TranscriptSegment>,
}

impl Default for RollingTranscriber {
    /// A transcriber with the [`DEFAULT_WINDOW_SECS`] / [`DEFAULT_OVERLAP_SECS`]
    /// window shape.
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW_SECS, DEFAULT_OVERLAP_SECS)
    }
}

impl RollingTranscriber {
    /// Builds a transcriber with a custom window length and overlap (both in
    /// seconds). See [`RollingTranscriber::default`] for the usual values.
    pub fn new(window_secs: f32, overlap_secs: f32) -> Self {
        Self {
            window_secs,
            overlap_secs,
            accumulated: Vec::new(),
        }
    }

    /// How far the window's start advances between consecutive calls: the
    /// window length minus the overlap, floored at `0.0`. Callers iterating a
    /// buffer should step by this and must ensure it is positive (an overlap
    /// at least as long as the window would otherwise never make progress).
    pub fn step_secs(&self) -> f32 {
        (self.window_secs - self.overlap_secs).max(0.0)
    }

    /// The stitched segments accumulated so far.
    pub fn segments(&self) -> &[TranscriptSegment] {
        &self.accumulated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, start_secs: f32, end_secs: f32) -> TranscriptSegment {
        TranscriptSegment {
            text: text.to_string(),
            start_secs,
            end_secs,
            speaker: None,
        }
    }

    #[test]
    fn stitch_offsets_window_segments_into_absolute_time() {
        let window = [seg("hello", 0.0, 1.0), seg("world", 1.0, 2.0)];
        let out = stitch(&[], &window, 10.0);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].start_secs, out[0].end_secs), (10.0, 11.0));
        assert_eq!((out[1].start_secs, out[1].end_secs), (11.0, 12.0));
    }

    #[test]
    fn stitch_drops_the_overlapping_seam_duplicate() {
        let existing = [seg("hello", 0.0, 1.0), seg("world", 1.0, 2.0)];
        // Window starts at 1.5s and re-transcribes the tail "world" (its own
        // 0.0..0.5 -> absolute 1.5..2.0) before new content "again".
        let window = [seg("World.", 0.0, 0.5), seg("again", 0.5, 1.5)];
        let out = stitch(&existing, &window, 1.5);

        let texts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["hello", "world", "again"]);
        // "again" landed at its absolute offset, not duplicated "world".
        assert_eq!((out[2].start_secs, out[2].end_secs), (2.0, 3.0));
    }

    #[test]
    fn stitch_result_is_monotonic_even_when_a_window_reaches_back() {
        let existing = [seg("a", 0.0, 5.0)];
        // A backwards-timed window segment must not make the timeline regress.
        let window = [seg("b", -3.0, -1.0)];
        let out = stitch(&existing, &window, 0.0);
        assert!(out[1].start_secs >= out[0].start_secs);
        assert!(out[1].end_secs >= out[1].start_secs);
    }

    #[test]
    fn stitch_with_empty_window_returns_existing_unchanged() {
        let existing = [seg("a", 0.0, 1.0)];
        let out = stitch(&existing, &[], 4.0);
        assert_eq!(out, existing);
    }

    #[test]
    fn stitch_with_zero_overlap_keeps_adjacent_segments() {
        let existing = [seg("a", 0.0, 2.0)];
        // Window starts exactly where existing ends: adjacent, not overlapping,
        // so nothing is deduped even though times touch.
        let window = [seg("a", 0.0, 2.0)];
        let out = stitch(&existing, &window, 2.0);
        assert_eq!(out.len(), 2);
        assert_eq!((out[1].start_secs, out[1].end_secs), (2.0, 4.0));
    }
}
