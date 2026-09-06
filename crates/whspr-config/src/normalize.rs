//! Toggles for whspr-refine's rule-based text normalization (numbers/dates/
//! times written as digits in a unified format, as opposed to the LLM
//! cleanup refiners do), plus the AJ-01/AJ-02 macro-expansion table.
//!
//! Defined here rather than alongside `Config`'s other settings structs in
//! `lib.rs` (like `SpeakerSettings`) so that crate's file stays under this
//! project's 600-line-per-file guideline (AA-06) -- same reasoning
//! `autostart.rs`/`speaker.rs` already follow.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Each rule toggle is independently switchable so a user who wants LLM
/// cleanup but not, say, forced digit-dates can turn just that one off. All
/// on by default. `macros` is not a toggle but a lookup table -- see its
/// own doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct NormalizeSettings {
    /// Spell out number words ("twenty five", "двадцать пять") as digits.
    pub numbers: bool,
    /// Normalize recognized date expressions to `YYYY-MM-DD`.
    pub dates: bool,
    /// Normalize recognized time expressions to 24-hour `HH:MM`.
    pub times: bool,
    /// Emacs-abbrev-style macro table (AJ-01/AJ-02): trigger phrase ->
    /// expansion text. A dictation containing a trigger phrase (matched
    /// case-insensitively, whole-phrase -- never inside a larger word) has
    /// it replaced with the expansion. Keyed exactly as spoken (e.g. "my
    /// email"), read from the config file's `[normalize.macros]` table.
    /// Empty by default, so a user who never defines any macros sees no
    /// behavior change.
    pub macros: BTreeMap<String, String>,
}

impl Default for NormalizeSettings {
    fn default() -> Self {
        Self {
            numbers: true,
            dates: true,
            times: true,
            macros: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_settings_defaults_all_on() {
        assert_eq!(
            NormalizeSettings::default(),
            NormalizeSettings {
                numbers: true,
                dates: true,
                times: true,
                macros: BTreeMap::new(),
            }
        );
    }

    // `Config`/`load_from` live in the crate root, not this module, but
    // these two exercise `NormalizeSettings` through them the same way
    // `lib.rs`'s test module does for `SpeakerSettings`.
    use crate::{load_from, Config};

    #[test]
    fn normalize_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.normalize.numbers = false;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.normalize, cfg.normalize);
    }

    #[test]
    fn load_from_toml_file_sets_normalize_toggles() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[normalize]").expect("failed to write normalize header");
        writeln!(file, "numbers = false").expect("failed to write numbers");
        writeln!(file, "dates = false").expect("failed to write dates");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.normalize.numbers);
        assert!(!cfg.normalize.dates);
        assert!(cfg.normalize.times); // not set in the file - stays default
    }
}
