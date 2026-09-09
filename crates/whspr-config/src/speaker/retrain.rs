//! Adaptive per-pair separation margins and the reversible reassignment that
//! feeds them. Split out of `super` (the speaker DB core) to keep each file
//! well under the AA-06 size cap.
//!
//! The margin is the "learned" half of retraining voiceprints: every time a
//! user corrects an attribution ([`SpeakerDb::reassign`]), the confused pair
//! of speakers has its required separation nudged up. `match_or_enroll` then
//! raises the acceptance threshold between the top-2 candidate speakers by
//! the learned margin, so a near-duplicate pair that used to cross-match
//! splits into distinct speakers over successive corrections. Growth is
//! bounded by [`MARGIN_CAP`] so the adaptation stays gentle.

use super::SpeakerDb;

/// How much one correction raises a confused pair's separation margin.
const MARGIN_STEP: f32 = 0.05;

/// The most a learned margin can grow, no matter how many corrections. Keeps
/// the effective threshold from running past what cosine similarity can ever
/// satisfy, and keeps the adaptation gentle and bounded.
const MARGIN_CAP: f32 = 0.25;

/// Identifies one stored [`super::TurnEmbedding`] within a profile, for
/// reassignment: the `index`-th turn (0-based) among that profile's turns
/// bearing this `scan_id`. Pairing `scan_id` with an occurrence index keeps
/// the reference unambiguous even when a single scan contributed several
/// turns (the diarization case, where every turn shares the source file path
/// as its scan id), and stable across the removal of turns from *other*
/// scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRef {
    pub scan_id: String,
    pub index: usize,
}

/// Normalizes an unordered pair of speaker ids into a single stable map key
/// (`"min|max"`), so a pair addresses the same entry regardless of the order
/// its two ids are passed in.
pub(super) fn margin_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}|{b}")
    } else {
        format!("{b}|{a}")
    }
}

impl SpeakerDb {
    /// The learned separation margin between two speakers, or `0.0` if the
    /// pair has never been corrected. Order-independent.
    pub fn margin_between(&self, a: &str, b: &str) -> f32 {
        self.margins.get(&margin_key(a, b)).copied().unwrap_or(0.0)
    }

    /// Reassigns a mis-attributed turn from one speaker to another: the
    /// user-facing "retrain the voiceprints" correction. Detaches the turn
    /// identified by `turn` from `from_id`, attaches it to `to_id`,
    /// recomputes *both* centroids from their now-updated turns (the
    /// reversible re-estimation a running mean could never do), and nudges
    /// the confused pair's adaptive margin up so the two split more
    /// aggressively on future matches.
    ///
    /// Returns `false` — changing nothing — if `from_id` and `to_id` are the
    /// same, if either id is unknown, or if `turn` matches no stored turn on
    /// the source profile.
    pub fn reassign(&mut self, from_id: &str, to_id: &str, turn: &TurnRef) -> bool {
        if from_id == to_id
            || !self.profiles.iter().any(|p| p.id == from_id)
            || !self.profiles.iter().any(|p| p.id == to_id)
        {
            return false;
        }

        // Detach the referenced turn from the source profile, recomputing
        // its centroid over what remains.
        let moved = {
            let Some(from) = self.profiles.iter_mut().find(|p| p.id == from_id) else {
                return false;
            };
            let Some(pos) = from
                .turns
                .iter()
                .enumerate()
                .filter(|(_, t)| t.scan_id == turn.scan_id)
                .nth(turn.index)
                .map(|(i, _)| i)
            else {
                return false;
            };
            let moved = from.turns.remove(pos);
            from.recompute_centroid();
            moved
        };

        // Attach it to the destination profile and recompute its centroid.
        if let Some(to) = self.profiles.iter_mut().find(|p| p.id == to_id) {
            to.turns.push(moved);
            to.recompute_centroid();
        }

        self.bump_margin(from_id, to_id);
        true
    }

    /// Raises the confused pair's learned margin by [`MARGIN_STEP`], saturating
    /// at [`MARGIN_CAP`] so it can't grow without bound.
    fn bump_margin(&mut self, a: &str, b: &str) {
        let entry = self.margins.entry(margin_key(a, b)).or_insert(0.0);
        *entry = (*entry + MARGIN_STEP).min(MARGIN_CAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "length mismatch: {actual:?} vs {expected:?}"
        );
        for (x, y) in actual.iter().zip(expected) {
            assert!(
                (x - y).abs() < 1e-6,
                "expected ~{expected:?}, got {actual:?}"
            );
        }
    }

    fn tref(scan_id: &str, index: usize) -> TurnRef {
        TurnRef {
            scan_id: scan_id.to_string(),
            index,
        }
    }

    #[test]
    fn reassign_moves_a_turn_and_recomputes_both_centroids() {
        let thr = 0.7;
        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&[1.0, 0.0, 0.0], thr, "sa");
        let (b, _) = db.match_or_enroll(&[0.0, 1.0, 0.0], thr, "sb");
        // A near-A turn cross-matches onto A, so A now holds two turns.
        db.match_or_enroll(&[0.9, 0.1, 0.0], thr, "mix");
        assert_eq!(db.profiles.iter().find(|p| p.id == a).unwrap().turns.len(), 2);

        // Correct: the "mix" turn actually belonged to B.
        assert!(db.reassign(&a, &b, &tref("mix", 0)));

        let a_prof = db.profiles.iter().find(|p| p.id == a).unwrap();
        let b_prof = db.profiles.iter().find(|p| p.id == b).unwrap();
        assert_eq!(a_prof.turns.len(), 1);
        assert_close(&a_prof.centroid, &[1.0, 0.0, 0.0]);
        assert_eq!(b_prof.turns.len(), 2);
        assert_close(&b_prof.centroid, &[0.45, 0.55, 0.0]);
        assert!(db.margin_between(&a, &b) > 0.0, "the correction records a margin");
    }

    #[test]
    fn reassign_returns_false_and_changes_nothing_on_bad_input() {
        let thr = 0.7;
        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&[1.0, 0.0], thr, "sa");
        let (b, _) = db.match_or_enroll(&[0.0, 1.0], thr, "sb");

        assert!(!db.reassign(&a, &a, &tref("sa", 0)), "self-reassignment is rejected");
        assert!(!db.reassign(&a, "nope", &tref("sa", 0)), "unknown destination");
        assert!(!db.reassign("nope", &b, &tref("sa", 0)), "unknown source");
        assert!(!db.reassign(&a, &b, &tref("no-such-scan", 0)), "unknown turn");
        assert!(!db.reassign(&a, &b, &tref("sa", 9)), "index past the end");
        assert!(db.margins.is_empty(), "no failed attempt recorded a margin");
    }

    #[test]
    fn a_correction_separates_a_pair_that_previously_cross_matched() {
        let thr = 0.7;
        let e_a = vec![1.0, 0.0];
        let e_b = vec![0.0, 1.0];
        // ~41 deg off A on the far side from B (cos ~0.755 to A): A stays the
        // clear best candidate even after B drifts toward A from the moved
        // turns, so it's the *margin* -- not B drift -- that flips the result.
        let e_dup = vec![0.7547, -0.6561];

        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&e_a, thr, "sa");
        let (b, _) = db.match_or_enroll(&e_b, thr, "sb");
        // Two more A turns available to be corrected onto B.
        db.match_or_enroll(&e_a, thr, "sa1");
        db.match_or_enroll(&e_a, thr, "sa2");

        // Before any correction, e_dup cross-matches onto A.
        let (matched, is_new) = db.clone().match_or_enroll(&e_dup, thr, "sc");
        assert_eq!(matched, a);
        assert!(!is_new, "before correction the near-duplicate merges into A");

        // The user corrects: those two turns were actually speaker B.
        assert!(db.reassign(&a, &b, &tref("sa1", 0)));
        assert!(db.reassign(&a, &b, &tref("sa2", 0)));
        assert!(db.margin_between(&a, &b) > 0.0, "corrections raised the pair's margin");

        // Now the learned margin lifts A's bar above e_dup's score, so it
        // enrolls as a distinct speaker instead of merging into A.
        let (_, is_new2) = db.match_or_enroll(&e_dup, thr, "sc2");
        assert!(is_new2, "after correction the near-duplicate no longer merges into A");
    }

    #[test]
    fn the_margin_is_bounded_no_matter_how_many_corrections() {
        let thr = 0.7;
        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&[1.0, 0.0], thr, "sa");
        let (b, _) = db.match_or_enroll(&[0.0, 1.0], thr, "sb");
        for i in 0..100 {
            db.match_or_enroll(&[1.0, 0.0], thr, &format!("t{i}"));
        }
        for i in 0..100 {
            assert!(db.reassign(&a, &b, &tref(&format!("t{i}"), 0)));
        }

        let margin = db.margin_between(&a, &b);
        assert!(margin > 0.0);
        assert!(
            (margin - MARGIN_CAP).abs() < 1e-6,
            "100 corrections must saturate at the cap {MARGIN_CAP}, got {margin}"
        );
    }

    #[test]
    fn reassign_records_the_margin_under_an_order_independent_key() {
        let thr = 0.7;
        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&[1.0, 0.0], thr, "sa");
        let (b, _) = db.match_or_enroll(&[0.0, 1.0], thr, "sb");
        db.match_or_enroll(&[1.0, 0.0], thr, "x");
        db.match_or_enroll(&[0.0, 1.0], thr, "y");

        // Correct in both directions; both must land on the one sorted key.
        assert!(db.reassign(&a, &b, &tref("x", 0)));
        assert!(db.reassign(&b, &a, &tref("y", 0)));

        assert_eq!(db.margins.len(), 1, "both directions share a single key");
        assert!(db.margins.contains_key(&margin_key(&a, &b)));
        assert_eq!(db.margin_between(&a, &b), db.margin_between(&b, &a));
    }

    #[test]
    fn margin_between_is_zero_for_an_unknown_pair_and_order_independent() {
        let mut db = SpeakerDb::default();
        assert_eq!(db.margin_between("a", "b"), 0.0);

        db.margins.insert(margin_key("a", "b"), 0.1);
        assert_eq!(db.margin_between("a", "b"), 0.1);
        assert_eq!(
            db.margin_between("b", "a"),
            0.1,
            "the lookup key is order-independent"
        );
    }

    #[test]
    fn a_learned_margin_raises_the_matching_bar_between_the_top_two() {
        // A at [1,0], B at [0,1]. e_dup sits ~41 degrees off A (cos ~0.755
        // to A, ~0.656 to B): A is the clear best candidate, B the runner-up.
        let thr = 0.7;
        let e_a = vec![1.0, 0.0];
        let e_b = vec![0.0, 1.0];
        let e_dup = vec![0.755, 0.656];

        // Baseline: nothing learned yet, so e_dup comfortably merges into A.
        let mut db = SpeakerDb::default();
        let (a, _) = db.match_or_enroll(&e_a, thr, "sa");
        let (_b, _) = db.match_or_enroll(&e_b, thr, "sb");
        let (matched, is_new) = db.match_or_enroll(&e_dup, thr, "sc");
        assert_eq!(matched, a);
        assert!(!is_new, "with no learned margin the near-duplicate merges into A");

        // With a learned (A,B) margin the same embedding no longer clears the
        // raised bar for A, so it enrolls as a distinct speaker instead.
        let mut db2 = SpeakerDb::default();
        let (a2, _) = db2.match_or_enroll(&e_a, thr, "sa");
        let (b2, _) = db2.match_or_enroll(&e_b, thr, "sb");
        db2.margins.insert(margin_key(&a2, &b2), 0.1);
        let (_, is_new2) = db2.match_or_enroll(&e_dup, thr, "sc");
        assert!(is_new2, "the learned margin raised the bar, splitting the pair");
    }
}
