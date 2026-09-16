//! Pure decisions behind a live capture: which input device to open (and
//! the fallback when the configured one is gone), the `CaptureOptions` the
//! user's `[capture]` settings translate to, how a finished clip is
//! trimmed, and whether an idle preroll monitor should be listening. Kept
//! free of cpal so every branch is unit-testable without a microphone.

use whspr_audio::CaptureOptions;
use whspr_config::Config;

/// The input device a capture should open, after checking the configured
/// name against the devices actually connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceResolution {
    /// Device name to open; `None` opens the OS default input device.
    pub device: Option<String>,
    /// The configured device name when it is not connected right now --
    /// capture then falls back to the default device and the worker tells
    /// the user so (see `crate::devices::fallback_notice`).
    pub missing: Option<String>,
}

/// Resolves `[device].input_device` (`configured`) against `available` (as
/// from `whspr_audio::input_device_names`). An empty `available` list means
/// enumeration itself failed (no microphone permission yet, a host error)
/// rather than every device vanishing, so the configured name is kept and
/// `whspr_audio`'s own name resolution decides.
pub(super) fn resolve_device(configured: Option<&str>, available: &[String]) -> DeviceResolution {
    match configured {
        Some(name) if available.is_empty() || available.iter().any(|a| a == name) => {
            DeviceResolution {
                device: Some(name.to_string()),
                missing: None,
            }
        }
        Some(name) => DeviceResolution {
            device: None,
            missing: Some(name.to_string()),
        },
        None => DeviceResolution {
            device: None,
            missing: None,
        },
    }
}

/// The `CaptureOptions` for a capture on `device`: `[capture].input_gain` and
/// `[capture].noise_suppression` straight from `config`, plus `preroll` (16 kHz
/// mono samples an idle monitor kept from just before the hotkey press;
/// empty when there is none).
pub(super) fn capture_options(
    config: &Config,
    device: Option<String>,
    preroll: Vec<f32>,
) -> CaptureOptions {
    CaptureOptions {
        device,
        input_gain: config.capture.input_gain,
        noise_suppression: config.capture.noise_suppression,
        preroll,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn no_configured_device_opens_the_default() {
        let resolved = resolve_device(None, &names(&["Built-in Microphone"]));
        assert_eq!(resolved.device, None);
        assert_eq!(resolved.missing, None);
    }

    #[test]
    fn a_connected_configured_device_is_opened_by_name() {
        let resolved = resolve_device(Some("USB Mic"), &names(&["Built-in Microphone", "USB Mic"]));
        assert_eq!(resolved.device.as_deref(), Some("USB Mic"));
        assert_eq!(resolved.missing, None);
    }

    #[test]
    fn a_disconnected_configured_device_falls_back_and_is_reported() {
        let resolved = resolve_device(Some("USB Mic"), &names(&["Built-in Microphone"]));
        assert_eq!(resolved.device, None);
        assert_eq!(resolved.missing.as_deref(), Some("USB Mic"));
    }

    #[test]
    fn an_empty_device_list_keeps_the_configured_name() {
        let resolved = resolve_device(Some("USB Mic"), &[]);
        assert_eq!(resolved.device.as_deref(), Some("USB Mic"));
        assert_eq!(resolved.missing, None);
    }
}
