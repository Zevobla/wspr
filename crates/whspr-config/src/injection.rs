//! Settings for how transcribed text is injected into the focused app
//! (whspr-inject's `EnigoTextSink`).

use serde::{Deserialize, Serialize};

/// Text-injection tuning, read from the config file's `[injection]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InjectionSettings {
    /// Pause, in milliseconds, applied *before* the paste keystroke is sent,
    /// giving a slow-to-focus target app a moment to settle before the paste
    /// lands (AM-20). `0` (the default) disables the pause. This is distinct
    /// from whspr-inject's internal post-paste clipboard-restore timing.
    pub pre_paste_delay_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pre_paste_delay_is_zero() {
        assert_eq!(InjectionSettings::default().pre_paste_delay_ms, 0);
    }

    #[test]
    fn round_trips_through_toml_with_kebab_case_key() {
        let settings = InjectionSettings {
            pre_paste_delay_ms: 150,
        };
        let toml = toml::to_string(&settings).expect("serialize");
        assert!(
            toml.contains("pre-paste-delay-ms"),
            "expected kebab-case key, got: {toml}"
        );
        let back: InjectionSettings = toml::from_str(&toml).expect("deserialize");
        assert_eq!(back, settings);
    }

    #[test]
    fn missing_field_defaults_to_zero() {
        // An empty `[injection]` table falls back to the field default.
        let back: InjectionSettings = toml::from_str("").expect("deserialize empty table");
        assert_eq!(back.pre_paste_delay_ms, 0);
    }
}
