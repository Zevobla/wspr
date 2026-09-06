//! Input-device enumeration and selection for `start_capture_on_device`
//! (C-05: let a caller pick a specific microphone instead of always using
//! the OS default). Split out of `lib.rs` to keep that file under this
//! project's 600-line-per-file guideline.

use cpal::traits::HostTrait;

use whspr_core::Result;

/// Names of all available audio input devices (not just the default), in
/// host-reported order - for a UI picker like the Hub's device dropdown.
/// Never panics: any cpal error (no host, enumeration failure) degrades to
/// an empty list rather than propagating, since this is a UI-listing
/// convenience, not a hard requirement for capture to work.
///
/// cpal 0.18 dropped `Device::name()` in favor of `Display` (required by
/// `DeviceTrait`) - `to_string()` is the documented shortcut for just the
/// human-readable name.
pub fn input_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices.map(|device| device.to_string()).collect()
}

/// Finds `requested` among `available` device names (exact match),
/// returning its index. Pure and hardware-independent, unlike
/// `resolve_input_device` (which needs real cpal devices to enumerate) -
/// this is the part of device selection that's directly unit-testable.
fn find_matching_device_name(available: &[String], requested: &str) -> Option<usize> {
    available.iter().position(|name| name == requested)
}

/// Resolves `name` to a real input device. Falls back to the default
/// input device (logging a warning) if no device matches that name - a
/// renamed/unplugged device shouldn't hard-fail a live dictation turn,
/// the same "degrade gracefully instead of crashing" spirit as this
/// crate's other fallbacks.
pub(crate) fn resolve_input_device(host: &cpal::Host, name: &str) -> Result<cpal::Device> {
    let devices: Vec<cpal::Device> = host
        .input_devices()
        .map_err(|e| crate::mic_access_error("enumerate input devices", e))?
        .collect();
    let names: Vec<String> = devices.iter().map(|d| d.to_string()).collect();

    if let Some(idx) = find_matching_device_name(&names, name) {
        return Ok(devices
            .into_iter()
            .nth(idx)
            .expect("idx came from names, which is the same length as devices"));
    }

    tracing::warn!("input device {name:?} not found; falling back to the default input device");
    host.default_input_device()
        .ok_or_else(crate::no_input_device_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_device_names_does_not_panic() {
        // We can't assert on the actual device list in a sandboxed/headless
        // CI environment (there may be zero devices) - only that
        // enumerating them is safe to call and returns without panicking.
        let _ = input_device_names();
    }

    #[test]
    fn find_matching_device_name_returns_index_of_exact_match() {
        let names = vec!["Built-in Microphone".to_string(), "USB Headset".to_string()];
        assert_eq!(find_matching_device_name(&names, "USB Headset"), Some(1));
        assert_eq!(
            find_matching_device_name(&names, "Built-in Microphone"),
            Some(0)
        );
    }

    #[test]
    fn find_matching_device_name_returns_none_when_not_found() {
        let names = vec!["Built-in Microphone".to_string()];
        assert_eq!(
            find_matching_device_name(&names, "Nonexistent Device"),
            None
        );
    }

    #[test]
    fn find_matching_device_name_of_empty_list_returns_none() {
        assert_eq!(find_matching_device_name(&[], "Anything"), None);
    }

    #[test]
    fn find_matching_device_name_is_exact_not_substring() {
        let names = vec!["USB Headset".to_string()];
        // A partial/substring match must not count as found - callers
        // (e.g. the Hub's picker) round-trip exact names, and silently
        // matching the wrong device would be worse than falling back to
        // the default.
        assert_eq!(find_matching_device_name(&names, "USB"), None);
    }
}
