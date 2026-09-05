//! Persisted speaker-enrollment database: every distinct speaker discovered
//! across past diarization scans, matched by cosine similarity against a
//! running centroid embedding. Lives in its own `speakers.json` in the
//! platform data dir (see `whspr-app/src/history.rs` and `whspr-cli`'s
//! `save_to_history` for the sibling pattern this follows — JSONL there,
//! for an append-only log; a single JSON document here, since this is one
//! evolving collection that gets rewritten in place, not appended to).

use std::path::Path;

use serde::{Deserialize, Serialize};

use whspr_core::cosine_similarity;

/// One enrolled speaker: a running-average embedding centroid built up from
/// every turn matched to them so far, plus display metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerProfile {
    /// Stable identifier, e.g. "Speaker 3". Never changes once assigned —
    /// `name` is what the UI should prefer to display once it's set.
    pub id: String,
    /// User-assigned display name. `None` until the user renames this
    /// speaker via `SpeakerDb::rename`; until then, callers should fall
    /// back to displaying `id`.
    pub name: Option<String>,
    pub centroid: Vec<f32>,
    /// Number of turns folded into `centroid` so far (used to weight the
    /// running average on the next match).
    pub samples: u32,
    /// Identifiers of the scans (e.g. source file paths) this speaker has
    /// appeared in.
    pub scans: Vec<String>,
    pub first_seen: u64,
    pub last_seen: u64,
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
    /// brand new profile named "Speaker N" (N = current profile count + 1)
    /// and returns `(id, true)`.
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
            let id = format!("Speaker {}", self.profiles.len() + 1);
            self.profiles.push(SpeakerProfile {
                id: id.clone(),
                name: None,
                centroid: embedding.to_vec(),
                samples: 1,
                scans: vec![scan_id.to_string()],
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

    #[test]
    fn match_or_enroll_empty_db_creates_speaker_1() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        let (id, is_new) = db.match_or_enroll(&embedding, 0.7, "scan1");

        assert_eq!(id, "Speaker 1");
        assert!(is_new);
        assert_eq!(db.profiles.len(), 1);
        assert_eq!(db.profiles[0].id, "Speaker 1");
        assert_eq!(db.profiles[0].samples, 1);
        assert_eq!(db.profiles[0].scans, vec!["scan1".to_string()]);
    }

    #[test]
    fn match_or_enroll_identical_embedding_matches_existing() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        // First enrollment
        let (id1, is_new1) = db.match_or_enroll(&embedding, 0.7, "scan1");
        assert_eq!(id1, "Speaker 1");
        assert!(is_new1);

        // Second enrollment with identical embedding should match
        let (id2, is_new2) = db.match_or_enroll(&embedding, 0.7, "scan2");
        assert_eq!(id2, "Speaker 1");
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
        assert_eq!(id1, "Speaker 1");
        assert!(is_new1);

        // Orthogonal embedding should score 0.0, below threshold
        let (id2, is_new2) = db.match_or_enroll(&embedding2, 0.7, "scan1");
        assert_eq!(id2, "Speaker 2");
        assert!(is_new2);

        assert_eq!(db.profiles.len(), 2);
    }

    #[test]
    fn rename_existing_profile() {
        let mut db = SpeakerDb::default();
        let embedding = vec![1.0, 0.0, 0.0];

        let (id, _) = db.match_or_enroll(&embedding, 0.7, "scan1");
        assert_eq!(id, "Speaker 1");

        let renamed = db.rename("Speaker 1", "Alice");
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
        assert_eq!(loaded.profiles[0].id, "Speaker 1");
        assert_eq!(loaded.profiles[0].name, Some("Test Speaker".to_string()));
        assert_eq!(loaded.profiles[0].samples, 1);
        assert!(!loaded.profiles[0].centroid.is_empty());
    }
}
