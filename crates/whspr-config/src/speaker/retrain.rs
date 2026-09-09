//! Adaptive per-pair separation margins and the reversible reassignment that
//! feeds them. Split out of `super` (the speaker DB core) to keep each file
//! well under the AA-06 size cap.
//!
//! The margin is the "learned" half of retraining voiceprints: every time a
//! user corrects an attribution ([`SpeakerDb::reassign`]), the confused pair
//! of speakers has its required separation nudged up. `match_or_enroll` then
//! raises the acceptance threshold between the top-2 candidate speakers by
//! the learned margin, so a near-duplicate pair that used to cross-match
//! splits into distinct speakers over successive corrections.

use super::SpeakerDb;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
