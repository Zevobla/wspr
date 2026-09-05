//! Enumerates microphone input devices for the Hub's device picker.
//!
//! `whspr-audio` only exposes starting capture on the default input device;
//! it has no device-listing API. Rather than grow a shared crate's public
//! surface for one small list call, this talks to `cpal` directly.

use cpal::traits::HostTrait;

/// Returns the names of all available audio input devices, in host-reported
/// order. Never panics: any cpal error (no host, enumeration failure, a
/// device that fails to report its own name) is treated as "skip it" rather
/// than propagated, since this is purely a UI listing convenience.
///
/// cpal 0.18 dropped `Device::name()` in favor of `Display` (which the
/// `DeviceTrait` bound requires every device to implement) -- `to_string()`
/// is the documented shortcut for just the human-readable name.
pub fn list_input_device_names() -> Vec<String> {
    let host = cpal::default_host();

    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };

    devices.map(|device| device.to_string()).collect()
}

/// The name of the host's default input device, if any. Used to pre-select
/// it in the Hub's device picker.
pub fn default_input_device_name() -> Option<String> {
    Some(cpal::default_host().default_input_device()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_input_device_names_does_not_panic() {
        // We can't assert on the actual device list in a sandboxed/headless
        // CI environment (there may be zero devices) -- only that
        // enumerating them is safe to call and returns without panicking.
        let _ = list_input_device_names();
    }

    #[test]
    fn default_input_device_name_does_not_panic() {
        let _ = default_input_device_name();
    }
}
