//! Device and window tracking settings (C-05, AL-09, J-04): hotplug
//! detection for audio devices and active window recording. Defined in its
//! own file to keep `lib.rs` under this project's 600-line-per-file
//! guideline (AA-06).

use serde::{Deserialize, Serialize};

/// Settings for audio device and window tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DeviceSettings {
    /// The name of the input device the user last selected in the Hub, if
    /// any. `None` means "use the host's default input device". Persisted so
    /// the chosen microphone survives a restart.
    pub input_device: Option<String>,
    /// Whether to rescan the input-device list when a device is plugged or
    /// unplugged. Default true — most users want whspr to automatically
    /// pick up newly connected microphones.
    pub device_hotplug: bool,
    /// Whether to record the focused window's app name for per-app stats and
    /// styling. Default true — enables app-specific context for dictations.
    pub active_window: bool,
    /// Whether to allow Bluetooth audio sources (e.g. wireless microphones,
    /// headsets). Default true — enables broad device compatibility.
    pub bluetooth_source: bool,
    /// Whether to allow virtual audio sources (e.g. software loopback,
    /// virtual mics). Default true — enables use with screenshare and
    /// streaming tools.
    pub virtual_source: bool,
    /// Whether the tray icon remains static and doesn't flicker on recording
    /// state changes. Default true — reduces visual noise for always-visible
    /// tray icons.
    pub tray_static: bool,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            device_hotplug: true,
            active_window: true,
            bluetooth_source: true,
            virtual_source: true,
            tray_static: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{load_from, Config};

    #[test]
    fn device_settings_defaults_all_true() {
        assert_eq!(
            DeviceSettings::default(),
            DeviceSettings {
                input_device: None,
                device_hotplug: true,
                active_window: true,
                bluetooth_source: true,
                virtual_source: true,
                tray_static: true,
            }
        );
    }

    #[test]
    fn device_settings_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.device.device_hotplug = false;
        cfg.device.active_window = false;

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(round_tripped.device, cfg.device);
    }

    #[test]
    fn load_from_toml_file_sets_device_settings() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[device]").expect("failed to write device header");
        writeln!(file, "device-hotplug = false").expect("failed to write device-hotplug");
        writeln!(file, "active-window = false").expect("failed to write active-window");
        writeln!(file, "bluetooth-source = false").expect("failed to write bluetooth-source");
        writeln!(file, "virtual-source = false").expect("failed to write virtual-source");
        writeln!(file, "tray-static = false").expect("failed to write tray-static");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.device.device_hotplug);
        assert!(!cfg.device.active_window);
        assert!(!cfg.device.bluetooth_source);
        assert!(!cfg.device.virtual_source);
        assert!(!cfg.device.tray_static);
    }

    #[test]
    fn device_settings_input_device_round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.device.input_device = Some("Built-in Microphone".to_string());

        let toml_string = toml::to_string_pretty(&cfg).expect("failed to serialize config");
        let round_tripped: Config =
            toml::from_str(&toml_string).expect("failed to deserialize config");

        assert_eq!(
            round_tripped.device.input_device,
            Some("Built-in Microphone".to_string())
        );
    }

    #[test]
    fn device_settings_input_device_defaults_to_none() {
        assert_eq!(DeviceSettings::default().input_device, None);
    }

    #[test]
    fn device_settings_partial_toml_merges_with_defaults() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");
        let mut file = std::fs::File::create(&config_path).expect("failed to create config.toml");
        writeln!(file, "[device]").expect("failed to write device header");
        writeln!(file, "device-hotplug = false").expect("failed to write device-hotplug");
        drop(file);

        let cfg = load_from(Some(temp_dir.path()));
        assert!(!cfg.device.device_hotplug);
        assert!(cfg.device.active_window); // not set in file - stays default
    }
}
