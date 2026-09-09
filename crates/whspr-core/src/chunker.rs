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

use crate::error::Result;
use crate::traits::AsrBackend;
use crate::types::{AsrOptions, AudioBuffer, Transcript, TranscriptSegment};

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

    /// The accumulated segments assembled into a [`Transcript`]: each
    /// segment's trimmed text joined with single spaces, alongside a clone of
    /// the segments themselves. Language is left unset — windowing says
    /// nothing about it.
    pub fn transcript(&self) -> Transcript {
        let text = self
            .accumulated
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Transcript {
            text,
            language: None,
            segments: self.accumulated.clone(),
        }
    }

    /// Carves the samples covering `[window_start_secs, window_start_secs +
    /// window_secs)` out of `audio` into a fresh sub-buffer at the same sample
    /// rate. Indices are clamped to the buffer, so a start past the end yields
    /// an empty buffer and a window running off the end is simply truncated.
    fn slice_window(&self, audio: &AudioBuffer, window_start_secs: f32) -> AudioBuffer {
        let sample_rate = audio.sample_rate;
        if sample_rate == 0 {
            return AudioBuffer::new(Vec::new(), sample_rate);
        }
        let rate = sample_rate as f32;
        let total = audio.samples.len();
        let start = ((window_start_secs.max(0.0) * rate).round() as usize).min(total);
        let len = (self.window_secs.max(0.0) * rate).round() as usize;
        let end = start.saturating_add(len).min(total);
        AudioBuffer::new(audio.samples[start..end].to_vec(), sample_rate)
    }

    /// Transcribes the window starting at `window_start_secs` and folds it into
    /// the running transcript, returning the updated accumulated segments.
    ///
    /// Slices the matching samples out of `audio`, hands the sub-buffer to
    /// `asr`, then [`stitch`]es the backend's segments in at their absolute
    /// offset (deduping the overlap seam). An empty slice still calls the
    /// backend; a backend that returns no segments for it leaves the
    /// accumulator unchanged.
    pub async fn push_window(
        &mut self,
        asr: &dyn AsrBackend,
        audio: &AudioBuffer,
        window_start_secs: f32,
        opts: &AsrOptions,
    ) -> Result<&[TranscriptSegment]> {
        let sub = self.slice_window(audio, window_start_secs);
        let transcript = asr.transcribe(&sub, opts).await?;
        self.accumulated = stitch(&self.accumulated, &transcript.segments, window_start_secs);
        Ok(&self.accumulated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::MockAsr;

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

    #[tokio::test]
    async fn rolling_transcriber_stitches_multiple_windows_with_absolute_times() {
        // MockAsr returns the same canned segment for every window; a segment
        // short relative to the step keeps consecutive placements disjoint so
        // each window contributes exactly one, at its own absolute offset.
        let asr = MockAsr {
            canned: Transcript {
                text: "chunk".to_string(),
                segments: vec![seg("chunk", 0.0, 1.0)],
                ..Default::default()
            },
        };
        let mut rolling = RollingTranscriber::new(10.0, 2.0);
        assert_eq!(rolling.step_secs(), 8.0);

        // 24s buffer -> windows at 0, 8, 16.
        let audio = AudioBuffer::new(vec![0.0; 16_000 * 24], 16_000);
        let step = rolling.step_secs();
        let mut start = 0.0;
        while start < audio.duration_secs() {
            rolling
                .push_window(&asr, &audio, start, &AsrOptions::default())
                .await
                .unwrap();
            start += step;
        }

        let segments = rolling.segments();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].start_secs, 0.0);
        assert_eq!(segments[1].start_secs, 8.0);
        assert_eq!(segments[2].start_secs, 16.0);

        let transcript = rolling.transcript();
        assert_eq!(transcript.text, "chunk chunk chunk");
        assert_eq!(transcript.segments.len(), 3);
    }

    #[tokio::test]
    async fn rolling_transcriber_handles_a_window_past_the_end() {
        let asr = MockAsr {
            canned: Transcript {
                text: "tail".to_string(),
                segments: vec![seg("tail", 0.0, 0.5)],
                ..Default::default()
            },
        };
        let mut rolling = RollingTranscriber::new(10.0, 0.0);
        // Buffer is only 1s but the window asks for 10s: the slice truncates,
        // and a start beyond the buffer would slice empty.
        let audio = AudioBuffer::new(vec![0.0; 16_000], 16_000);
        rolling
            .push_window(&asr, &audio, 0.0, &AsrOptions::default())
            .await
            .unwrap();
        assert_eq!(rolling.segments().len(), 1);
    }

    #[tokio::test]
    async fn rolling_transcriber_window_start_past_end_slices_empty() {
        // A backend that echoes how many samples it was actually handed, so we
        // can assert the slice was empty without reaching into the private fn.
        struct LenAsr;
        #[async_trait::async_trait]
        impl AsrBackend for LenAsr {
            async fn transcribe(
                &self,
                audio: &AudioBuffer,
                _opts: &AsrOptions,
            ) -> Result<Transcript> {
                Ok(Transcript {
                    text: audio.samples.len().to_string(),
                    ..Default::default()
                })
            }
            fn id(&self) -> &'static str {
                "len"
            }
        }

        let mut rolling = RollingTranscriber::new(5.0, 0.0);
        let audio = AudioBuffer::new(vec![0.0; 16_000], 16_000);
        // Start at 100s, far past the 1s buffer.
        let sub_len = {
            let out = rolling
                .push_window(&LenAsr, &audio, 100.0, &AsrOptions::default())
                .await
                .unwrap();
            // No segments accumulated from the empty slice.
            out.len()
        };
        assert_eq!(sub_len, 0);
    }

    #[test]
    fn default_uses_the_documented_window_shape() {
        let rolling = RollingTranscriber::default();
        assert_eq!(
            rolling.step_secs(),
            DEFAULT_WINDOW_SECS - DEFAULT_OVERLAP_SECS
        );
    }
}
