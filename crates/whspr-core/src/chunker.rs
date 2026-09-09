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
