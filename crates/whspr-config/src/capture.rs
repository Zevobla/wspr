//! Capture and transcript handling settings (AM-18, AH-01, AJ-09): timeout,
//! auto-send, and input field detection. Defined in its own file to keep
//! lib.rs under this project's 600-line-per-file guideline (AA-06).

use serde::{Deserialize, Serialize};

/// Settings for transcription capture, refinement, and injection behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CaptureSettings {
    /// Maximum time in milliseconds to wait for the refine step (text cleanup)
    /// to complete. Default 30000 (30 seconds) — longer timeouts allow more
    /// comprehensive cleanup but delay the final transcript.
    pub refine_timeout_ms: u64,
    /// Whether to automatically inject the transcript when recording pauses.
    /// Default false — users typically review before injecting, but some prefer
    /// hands-free continuous dictation.
    pub auto_send: bool,
    /// Whether whspr auto-detects input fields before injection to avoid
    /// sending text to read-only contexts. Default true — prevents accidental
    /// paste-mode failures in unsupported apps.
    pub input_field_detection: bool,
    /// Whether to apply noise suppression preprocessing to audio (C-16).
    /// Default false — can improve clarity in noisy environments but may alter
    /// original audio quality.
    pub noise_suppression: bool,
    /// Input gain multiplier for audio capture (C-09). Default 1.0 (no change)
    /// — increase for quiet sources, decrease for loud sources.
    pub input_gain: f32,
    /// Voice activity detection (VAD) threshold (E-04). Default 0.01 — lower
    /// values are more sensitive to detecting voice, higher values require
    /// louder audio to trigger recording.
    pub vad_threshold: f32,
    /// Whether to translate transcribed text to a different language (J-10).
    /// Default false — requires a translation backend to be configured.
    pub translate: bool,
    /// Whether to shorten the transcript text (J-11). Default false — applies
    /// text summarization or compression when enabled.
    pub shorten: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            refine_timeout_ms: 30000,
            auto_send: false,
            input_field_detection: true,
            noise_suppression: false,
            input_gain: 1.0,
            vad_threshold: 0.01,
            translate: false,
            shorten: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_from, Config};

    #[test]
    fn capture_settings_defaults() {
        assert_eq!(
            CaptureSettings::default(),
            CaptureSettings {
                refine_timeout_ms: 30000,
                auto_send: false,
                input_field_detection: true,
                noise_suppression: false,
                input_gain: 1.0,
                vad_threshold: 0.01,
                translate: false,
                shorten: false,
            }
        );
    }

    #[test]
    fn capture_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.capture.refine_timeout_ms = 60000;
        cfg.capture.auto_send = true;
        cfg.capture.input_field_detection = false;
        cfg.capture.noise_suppression = true;
        cfg.capture.input_gain = 1.5;
        cfg.capture.vad_threshold = 0.05;
        cfg.capture.translate = true;
        cfg.capture.shorten = true;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.capture, cfg.capture);
    }

    #[test]
    fn load_from_toml_file_sets_capture_settings() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[capture]").expect("failed to write capture header");
        writeln!(file, "refine-timeout-ms = 60000").expect("failed to write refine-timeout-ms");
        writeln!(file, "auto-send = true").expect("failed to write auto-send");
        writeln!(file, "input-field-detection = false")
            .expect("failed to write input-field-detection");
        writeln!(file, "noise-suppression = true").expect("failed to write noise-suppression");
        writeln!(file, "input-gain = 1.5").expect("failed to write input-gain");
        writeln!(file, "vad-threshold = 0.05").expect("failed to write vad-threshold");
        writeln!(file, "translate = true").expect("failed to write translate");
        writeln!(file, "shorten = true").expect("failed to write shorten");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.capture.refine_timeout_ms, 60000);
        assert!(cfg.capture.auto_send);
        assert!(!cfg.capture.input_field_detection);
        assert!(cfg.capture.noise_suppression);
        assert_eq!(cfg.capture.input_gain, 1.5);
        assert_eq!(cfg.capture.vad_threshold, 0.05);
        assert!(cfg.capture.translate);
        assert!(cfg.capture.shorten);
    }

    #[test]
    fn capture_settings_partial_toml_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[capture]").expect("failed to write capture header");
        writeln!(file, "auto-send = true").expect("failed to write auto-send");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert_eq!(cfg.capture.refine_timeout_ms, 30000); // not set - stays default
        assert!(cfg.capture.auto_send);
        assert!(cfg.capture.input_field_detection); // not set - stays default
        assert!(!cfg.capture.noise_suppression); // not set - stays default
        assert_eq!(cfg.capture.input_gain, 1.0); // not set - stays default
        assert_eq!(cfg.capture.vad_threshold, 0.01); // not set - stays default
        assert!(!cfg.capture.translate); // not set - stays default
        assert!(!cfg.capture.shorten); // not set - stays default
    }
}
