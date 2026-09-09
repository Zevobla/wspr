//! Persisted speaker-enrollment database: every distinct speaker discovered
//! across past diarization scans, matched by cosine similarity against a
//! centroid embedding. Lives in its own `speakers.json` in the platform
//! data dir (see `whspr-app/src/history.rs` and `whspr-cli`'s
//! `save_to_history` for the sibling pattern this follows — JSONL there,
//! for an append-only log; a single JSON document here, since this is one
//! evolving collection that gets rewritten in place, not appended to).
//!
//! Each profile's `centroid` is a *recomputed cache* over the individual
//! per-turn embeddings retained in `turns`, not a lossy one-way running
//! mean. Keeping every contributing turn is what makes attribution
//! *reversible*: a mis-assigned turn can be moved to the right speaker (see
//! [`SpeakerDb::reassign`] in the `retrain` submodule) and both centroids
//! recomputed exactly, which a running mean can never undo. Corrections
//! also teach an adaptive per-pair margin so confused speakers split more
//! aggressively over time.

use std::path::Path;

use serde::{Deserialize, Serialize};

use whspr_core::cosine_similarity;

/// One retained per-turn embedding contributing to a speaker's voiceprint.
///
/// Turns are kept individually rather than folded into a lossy running mean
/// so that a mis-attributed turn can be *moved* between speakers and both
/// centroids recomputed exactly — the reversibility [`SpeakerDb::reassign`]
/// depends on. `start_secs`/`end_secs` are optional because
/// [`SpeakerDb::match_or_enroll`] (the live-dictation path) doesn't always
/// know a turn's position within its source audio; diarization scans do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnEmbedding {
    #[serde(default)]
    pub embedding: Vec<f32>,
    /// The scan (source file path, or a per-dictation id) this turn came
    /// from. Half of a turn's [`TurnRef`] identity.
    #[serde(default)]
    pub scan_id: String,
    #[serde(default)]
    pub start_secs: Option<f32>,
    #[serde(default)]
    pub end_secs: Option<f32>,
}

/// One enrolled speaker: a centroid embedding recomputed as the mean of the
/// per-turn embeddings retained in `turns`, plus display metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerProfile {
    /// Stable primary key: a v4 UUID assigned at enrollment, stable forever
    /// once assigned. `name` is an optional display label the UI should
    /// prefer once it's set.
    pub id: String,
    /// User-assigned display name. `None` until the user renames this
    /// speaker via `SpeakerDb::rename`; until then, callers should fall
    /// back to displaying `id`.
    pub name: Option<String>,
    /// Derived cache: the mean of every embedding in `turns`, kept in sync
    /// by [`SpeakerProfile::recompute_centroid`]. When `turns` is empty —
    /// a legacy `speakers.json` written before per-turn retention — this is
    /// the last stored running-mean value and is used for matching as-is.
    pub centroid: Vec<f32>,
    /// Number of turns contributing to `centroid`. Equal to `turns.len()`
    /// once any turn has been retained; for a legacy profile (empty
    /// `turns`) it's the stored running-average sample count.
    pub samples: u32,
    /// Identifiers of the scans (e.g. source file paths) this speaker has
    /// appeared in.
    pub scans: Vec<String>,
    /// Every per-turn embedding attributed to this speaker. The centroid is
    /// recomputed from these; retaining them is what makes reassignment
    /// reversible. `#[serde(default)]` so legacy files (which lack this
    /// field) still load.
    #[serde(default)]
    pub turns: Vec<TurnEmbedding>,
    pub first_seen: u64,
    pub last_seen: u64,
}

impl SpeakerProfile {
    /// Recomputes `centroid` as the element-wise mean of every retained
    /// `TurnEmbedding`, and syncs `samples` to the retained-turn count. This
    /// is the derived-cache invariant: after any mutation of `turns`,
    /// `centroid` is exactly the mean of what remains.
    ///
    /// When `turns` is empty — the case for a legacy `speakers.json` written
    /// before per-turn embeddings were retained — the stored `centroid` and
    /// `samples` are left untouched, so old databases keep matching against
    /// their last running-mean value.
    pub fn recompute_centroid(&mut self) {
        if self.turns.is_empty() {
            return;
        }
        let dim = self.turns.iter().map(|t| t.embedding.len()).max().unwrap_or(0);
        let mut mean = vec![0.0f32; dim];
        for turn in &self.turns {
            for (m, e) in mean.iter_mut().zip(&turn.embedding) {
                *m += *e;
            }
        }
        let n = self.turns.len() as f32;
        for m in &mut mean {
            *m /= n;
        }
        self.centroid = mean;
        self.samples = self.turns.len() as u32;
    }
}

/// The persisted collection of every enrolled speaker discovered so far
/// across all past diarization scans.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeakerDb {
    #[serde(default)]
    pub profiles: Vec<SpeakerProfile>,
}

impl SpeakerDb {
    /// Matches `embedding` against every enrolled profile's centroid by
    /// cosine similarity. If the best match is `>= threshold`, folds this
    /// embedding into that profile's running centroid (a running mean
    /// weighted by its `samples` count so far), records `scan_id` if new,
    /// bumps `last_seen`, and returns `(id, false)`. Otherwise enrolls a
    /// brand-new profile with a fresh v4 UUID id and returns `(id, true)`.
    pub fn match_or_enroll(
        &mut self,
        embedding: &[f32],
        threshold: f32,
        scan_id: &str,
    ) -> (String, bool) {
        let now = now_unix();

        let best = self
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| (i, cosine_similarity(embedding, &p.centroid)))
            .filter(|(_, score)| *score >= threshold)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((i, _score)) = best {
            let profile = &mut self.profiles[i];
            let n = profile.samples as f32;
            for (c, e) in profile.centroid.iter_mut().zip(embedding) {
                *c = (*c * n + e) / (n + 1.0);
            }
            profile.samples += 1;
            profile.last_seen = now;
            if !profile.scans.iter().any(|s| s == scan_id) {
                profile.scans.push(scan_id.to_string());
            }
            (profile.id.clone(), false)
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            self.profiles.push(SpeakerProfile {
                id: id.clone(),
                name: None,
                centroid: embedding.to_vec(),
                samples: 1,
                scans: vec![scan_id.to_string()],
                turns: vec![TurnEmbedding {
                    embedding: embedding.to_vec(),
                    scan_id: scan_id.to_string(),
                    start_secs: None,
                    end_secs: None,
                }],
                first_seen: now,
                last_seen: now,
            });
            (id, true)
        }
    }

    /// Renames the profile with the given `id`. Returns `false` if no
    /// profile with that id exists.
    pub fn rename(&mut self, id: &str, name: impl Into<String>) -> bool {
        match self.profiles.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.name = Some(name.into());
                true
            }
            None => false,
        }
    }

    /// Loads the database from `path`, tolerating a missing or unreadable
    /// file (returns an empty db) since a fresh install won't have one yet.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes the database to `path` as pretty JSON, creating any missing
    /// parent directories first.
    pub fn save(&self, path: &Path) -> whspr_core::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| {
                whspr_core::WhsprError::Config(format!("failed to create speaker db dir: {e}"))
            })?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            whspr_core::WhsprError::Config(format!("failed to serialize speaker db: {e}"))
        })?;
        std::fs::write(path, json)
            .map_err(|e| whspr_core::WhsprError::Config(format!("failed to write speaker db: {e}")))
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A newly enrolled speaker gets a fresh v4 UUID as its id.
    fn assert_is_uuid(id: &str) {
        assert_eq!(id.len(), 36, "a v4 UUID string is 36 chars: {id}");
        assert!(id.contains('-'), "a UUID string is hyphenated: {id}");
        assert!(
            uuid::Uuid::parse_str(id).is_ok(),
            "id should parse as a UUID: {id}"
        );
    }

    #[test]
    fn match_or_enroll_empty_db_assigns_uuid() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        let (id, is_new) = db.match_or_enroll(&embedding, 0.7, "scan1");

        assert_is_uuid(&id);
        assert!(is_new);
        assert_eq!(db.profiles.len(), 1);
        assert_eq!(db.profiles[0].id, id);
        assert_eq!(db.profiles[0].samples, 1);
        assert_eq!(db.profiles[0].scans, vec!["scan1".to_string()]);
    }

    #[test]
    fn match_or_enroll_identical_embedding_matches_existing() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        // First enrollment
        let (id1, is_new1) = db.match_or_enroll(&embedding, 0.7, "scan1");
        assert_is_uuid(&id1);
        assert!(is_new1);

        // Second enrollment with identical embedding should match the same id
        let (id2, is_new2) = db.match_or_enroll(&embedding, 0.7, "scan2");
        assert_eq!(id2, id1);
        assert!(!is_new2);

        // Should have only one profile with updated samples and scans
        assert_eq!(db.profiles.len(), 1);
        assert_eq!(db.profiles[0].samples, 2);
        assert_eq!(
            db.profiles[0].scans,
            vec!["scan1".to_string(), "scan2".to_string()]
        );
    }

    #[test]
    fn match_or_enroll_orthogonal_embedding_creates_new_speaker() {
        let mut db = SpeakerDb::default();
        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];

        let (id1, is_new1) = db.match_or_enroll(&embedding1, 0.7, "scan1");
        assert_is_uuid(&id1);
        assert!(is_new1);

        // Orthogonal embedding should score 0.0, below threshold
        let (id2, is_new2) = db.match_or_enroll(&embedding2, 0.7, "scan1");
        assert_is_uuid(&id2);
        assert!(is_new2);
        assert_ne!(id2, id1);

        assert_eq!(db.profiles.len(), 2);
    }

    #[test]
    fn rename_existing_profile() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        let (id, _) = db.match_or_enroll(&embedding, 0.7, "scan1");
        assert_is_uuid(&id);

        let renamed = db.rename(&id, "Alice");
        assert!(renamed);
        assert_eq!(db.profiles[0].name, Some("Alice".to_string()));
    }

    #[test]
    fn rename_nonexistent_profile_returns_false() {
        let mut db = SpeakerDb::default();
        let renamed = db.rename("Nonexistent", "Name");
        assert!(!renamed);
    }

    #[test]
    fn load_nonexistent_path_returns_empty_db() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let nonexistent_path = temp_dir.path().join("nonexistent.json");

        let db = SpeakerDb::load(&nonexistent_path);
        assert_eq!(db.profiles.len(), 0);
    }

    #[test]
    fn save_then_load_round_trips() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = temp_dir.path().join("speakers.json");

        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];
        let (id, _) = db.match_or_enroll(&embedding, 0.7, "scan1");
        db.rename(&id, "Test Speaker");

        db.save(&db_path).expect("save should succeed");

        let loaded = SpeakerDb::load(&db_path);
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].id, id);
        assert_eq!(loaded.profiles[0].name, Some("Test Speaker".to_string()));
        assert_eq!(loaded.profiles[0].samples, 1);
        assert!(!loaded.profiles[0].centroid.is_empty());
    }

    fn turn(embedding: Vec<f32>, scan_id: &str) -> TurnEmbedding {
        TurnEmbedding {
            embedding,
            scan_id: scan_id.to_string(),
            start_secs: None,
            end_secs: None,
        }
    }

    #[test]
    fn recompute_centroid_is_the_mean_of_the_retained_turns() {
        let mut profile = SpeakerProfile {
            id: "id".to_string(),
            name: None,
            centroid: Vec::new(),
            samples: 0,
            scans: Vec::new(),
            turns: vec![
                turn(vec![0.0, 0.0, 2.0], "a"),
                turn(vec![2.0, 0.0, 0.0], "b"),
            ],
            first_seen: 0,
            last_seen: 0,
        };

        profile.recompute_centroid();

        assert_eq!(profile.centroid, vec![1.0, 0.0, 1.0]);
        assert_eq!(profile.samples, 2, "samples tracks the retained-turn count");
    }

    #[test]
    fn recompute_centroid_keeps_the_stored_centroid_when_turns_is_empty() {
        // A legacy profile: a stored running-mean centroid but no retained
        // turns. Recompute must leave it (and its sample count) untouched so
        // old databases keep matching.
        let mut profile = SpeakerProfile {
            id: "legacy".to_string(),
            name: None,
            centroid: vec![0.5, 0.5, 0.0],
            samples: 42,
            scans: vec!["old".to_string()],
            turns: Vec::new(),
            first_seen: 0,
            last_seen: 0,
        };

        profile.recompute_centroid();

        assert_eq!(profile.centroid, vec![0.5, 0.5, 0.0]);
        assert_eq!(profile.samples, 42);
    }

    #[test]
    fn turn_embedding_fields_default_when_absent_from_json() {
        // Only `embedding` present; the times and scan id fall back to their
        // serde defaults.
        let turn: TurnEmbedding =
            serde_json::from_str(r#"{"embedding":[1.0,2.0]}"#).expect("should deserialize");
        assert_eq!(turn.embedding, vec![1.0, 2.0]);
        assert_eq!(turn.scan_id, "");
        assert_eq!(turn.start_secs, None);
        assert_eq!(turn.end_secs, None);
    }
}
