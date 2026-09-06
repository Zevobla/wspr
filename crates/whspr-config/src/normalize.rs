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

/// How normalized numbers/dates/times are rendered in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumberFormat {
    /// Render as digits (e.g., "25", "2025-09-06").
    #[default]
    Digits,
    /// Render as words (e.g., "twenty five", "twenty twenty five").
    Words,
}

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
    /// How normalized numbers/dates/times are rendered. See `NumberFormat`.
    pub numbers_format: NumberFormat,
    /// Emacs-abbrev-style macro table (AJ-01/AJ-02): trigger phrase ->
    /// expansion text. A dictation containing a trigger phrase (matched
    /// case-insensitively, whole-phrase -- never inside a larger word) has
    /// it replaced with the expansion. Keyed exactly as spoken (e.g. "my
    /// email"), read from the config file's `[normalize.macros]` table.
    /// Empty by default, so a user who never defines any macros sees no
    /// behavior change.
    pub macros: BTreeMap<String, String>,
    /// Whether to insert paragraph breaks on long pauses (G-09). Default true
    /// — helps structure long-form dictation with clear paragraph boundaries.
    pub paragraph_break: bool,
    /// Whether to automatically apply punctuation normalization (G-16).
    /// Default true — controls on/off toggle for auto-punctuation cleanup.
    pub punctuation_toggle: bool,
    /// Dictionary table for custom term replacements (H-01): trigger term ->
    /// replacement. Similar to `macros` but for finer-grained term-level
    /// substitution. Keyed exactly as recognized, read from the config file's
    /// `[normalize.dictionary]` table. Empty by default.
    pub dictionary: BTreeMap<String, String>,
}

impl Default for NormalizeSettings {
    fn default() -> Self {
        Self {
            numbers: true,
            dates: true,
            times: true,
            numbers_format: NumberFormat::Digits,
            macros: BTreeMap::new(),
            paragraph_break: true,
            punctuation_toggle: true,
            dictionary: BTreeMap::new(),
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
                numbers_format: NumberFormat::Digits,
                macros: BTreeMap::new(),
                paragraph_break: true,
                punctuation_toggle: true,
                dictionary: BTreeMap::new(),
            }
        );
    }

    #[test]
    fn numbers_format_defaults_to_digits() {
        assert_eq!(NumberFormat::default(), NumberFormat::Digits);
    }

    #[test]
    fn numbers_format_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.normalize.numbers_format = NumberFormat::Words;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.normalize.numbers_format, NumberFormat::Words);
    }

    #[test]
    fn load_from_toml_file_sets_numbers_format() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[normalize]").expect("failed to write normalize header");
        writeln!(file, "numbers-format = \"words\"").expect("failed to write numbers-format");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.normalize.numbers_format, NumberFormat::Words);
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

    #[test]
    fn macros_default_to_empty() {
        assert!(NormalizeSettings::default().macros.is_empty());
    }

    #[test]
    fn macros_round_trip_through_toml() {
        let mut cfg = Config::default();
        cfg.normalize
            .macros
            .insert("my email".to_string(), "me@example.com".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.normalize, cfg.normalize);
    }

    #[test]
    fn load_from_toml_file_sets_macros() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[normalize.macros]").expect("failed to write macros header");
        writeln!(file, "\"my email\" = \"me@example.com\"").expect("failed to write macro entry");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(
            cfg.normalize.macros.get("my email"),
            Some(&"me@example.com".to_string())
        );
    }

    #[test]
    fn normalize_settings_paragraph_break_and_punctuation_toggle_round_trip() {
        let mut cfg = Config::default();
        cfg.normalize.paragraph_break = false;
        cfg.normalize.punctuation_toggle = false;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert!(!round_tripped.normalize.paragraph_break);
        assert!(!round_tripped.normalize.punctuation_toggle);
    }

    #[test]
    fn normalize_settings_dictionary_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.normalize
            .dictionary
            .insert("tech-term".to_string(), "technical-replacement".to_string());
        cfg.normalize
            .dictionary
            .insert("common-abbr".to_string(), "abbreviation-expanded".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.normalize, cfg.normalize);
    }

    #[test]
    fn load_from_toml_file_sets_normalize_new_fields() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[normalize]").expect("failed to write normalize header");
        writeln!(file, "paragraph-break = false").expect("failed to write paragraph-break");
        writeln!(file, "punctuation-toggle = false").expect("failed to write punctuation-toggle");
        writeln!(file, "[normalize.dictionary]").expect("failed to write dictionary header");
        writeln!(file, "\"code-term\" = \"expanded-code\"").expect("failed to write dictionary entry");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.normalize.paragraph_break);
        assert!(!cfg.normalize.punctuation_toggle);
        assert_eq!(
            cfg.normalize.dictionary.get("code-term"),
            Some(&"expanded-code".to_string())
        );
    }
}
