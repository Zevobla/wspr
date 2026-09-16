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

/// The name of the host's default input device, if any - used to pre-select
/// it in a UI picker (e.g. the Hub's device dropdown) when the user hasn't
/// chosen one explicitly. Never panics: no default host/device is a normal,
/// expected outcome (e.g. a headless/sandboxed environment), not an error.
pub fn default_input_device_name() -> Option<String> {
    Some(cpal::default_host().default_input_device()?.to_string())
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
        .map_err(|e| crate::capture::mic_access_error("enumerate input devices", e))?
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
        .ok_or_else(crate::capture::no_input_device_error)
}

/// Case-insensitive substring markers for a Bluetooth input source
/// (headset/earbuds mic).
const BLUETOOTH_MARKERS: &[&str] = &["bluetooth", "airpods"];

/// Markers checked as a whole word rather than a plain substring (see
/// `contains_word`) - short enough ("bt") to otherwise false-positive
/// inside an unrelated device name (e.g. "Subtotal Device" contains the
/// substring "bt").
const BLUETOOTH_WORD_MARKERS: &[&str] = &["bt"];

/// Case-insensitive substring markers for a virtual/loopback input
/// source, not a physical microphone. Covers common virtual-audio
/// driver/tool names plus the generic terms "virtual" and "aggregate"
/// (macOS's Audio MIDI Setup "Aggregate Device" combines/loops back other
/// devices rather than being a mic itself).
const VIRTUAL_MARKERS: &[&str] = &[
    "virtual",
    "blackhole",
    "loopback",
    "soundflower",
    "vb-cable",
    "aggregate",
];

/// Whether `haystack_lower` (already lowercased) contains `word_lower` as
/// a standalone, whole "word" - tokenizing on any non-alphanumeric
/// character - rather than as a substring of a longer token.
fn contains_word(haystack_lower: &str, word_lower: &str) -> bool {
    haystack_lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == word_lower)
}

/// Whether `name` (an input device name, as from `input_device_names`)
/// looks like a Bluetooth source (a headset/earbuds mic), by a handful of
/// documented, case-insensitive name markers: "Bluetooth", "AirPods", and
/// the standalone word "BT".
///
/// This is a heuristic over the OS-reported device name - cpal exposes no
/// cross-platform "is this device Bluetooth" query - so it can both miss
/// a real Bluetooth device with an unusual name and (rarely) false-positive
/// on a device that merely happens to have one of these markers in its
/// name.
pub fn is_bluetooth_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    BLUETOOTH_MARKERS.iter().any(|m| lower.contains(m))
        || BLUETOOTH_WORD_MARKERS
            .iter()
            .any(|w| contains_word(&lower, w))
}

/// Whether `name` looks like a virtual/loopback audio source rather than
/// a physical microphone, by the same kind of documented name-marker
/// heuristic as `is_bluetooth_name`: common virtual-audio driver/tool
/// names ("BlackHole", "Soundflower", "VB-Cable"), "Loopback", and the
/// generic terms "virtual"/"aggregate". Same caveats as `is_bluetooth_name`
/// apply - this is a name heuristic, not a device-capability query.
pub fn is_virtual_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIRTUAL_MARKERS.iter().any(|m| lower.contains(m))
}

/// Filters `names` (as from `input_device_names`) down to the devices
/// allowed by the given policy - mirrors `whspr-config`'s `[device]
/// bluetooth_source`/`virtual_source` toggles (this crate doesn't depend
/// on `whspr-config`, so the caller reads those and passes them through).
/// A name matching neither heuristic (an ordinary physical/wired mic)
/// always passes through, regardless of either flag.
pub fn filter_input_devices(
    names: Vec<String>,
    allow_bluetooth: bool,
    allow_virtual: bool,
) -> Vec<String> {
    names
        .into_iter()
        .filter(|name| {
            (allow_bluetooth || !is_bluetooth_name(name))
                && (allow_virtual || !is_virtual_name(name))
        })
        .collect()
}

/// Which input device names appeared or disappeared between two
/// `DeviceWatcher` polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceChange {
    /// Names present in the latest poll but not the previous one.
    pub added: Vec<String>,
    /// Names present in the previous poll but not the latest one.
    pub removed: Vec<String>,
}

/// Pure diff between two device-name snapshots - the part of
/// `DeviceWatcher::poll` that's directly unit-testable with fixed name
/// lists, independent of real device enumeration or timing.
fn diff_device_names(before: &[String], after: &[String]) -> DeviceChange {
    DeviceChange {
        added: after
            .iter()
            .filter(|n| !before.contains(n))
            .cloned()
            .collect(),
        removed: before
            .iter()
            .filter(|n| !after.contains(n))
            .cloned()
            .collect(),
    }
}

/// Polls `input_device_names` on an interval and reports the diff since
/// the last poll - hotplug detection (`whspr-config`'s `[device]
/// device_hotplug`) without relying on OS-specific hotplug callbacks,
/// which cpal doesn't expose cross-platform. This is deliberately
/// polling, not a push notification.
pub struct DeviceWatcher {
    poll_interval: std::time::Duration,
    last_poll: std::time::Instant,
    known: Vec<String>,
}

impl DeviceWatcher {
    /// Creates a watcher seeded with the current device list, so the
    /// first `poll` reports only genuine changes rather than every
    /// currently-connected device as "added".
    pub fn new(poll_interval: std::time::Duration) -> Self {
        DeviceWatcher {
            poll_interval,
            last_poll: std::time::Instant::now(),
            known: input_device_names(),
        }
    }

    /// If at least `poll_interval` has elapsed since the last recompute,
    /// re-enumerates input devices and returns the diff against the
    /// previous poll (`None` if nothing changed). Before that interval
    /// has elapsed, returns `None` immediately without re-enumerating -
    /// cheap enough to call on every tick of a UI/event loop.
    pub fn poll(&mut self) -> Option<DeviceChange> {
        if self.last_poll.elapsed() < self.poll_interval {
            return None;
        }
        self.last_poll = std::time::Instant::now();

        let current = input_device_names();
        let change = diff_device_names(&self.known, &current);
        self.known = current;

        if change.added.is_empty() && change.removed.is_empty() {
            None
        } else {
            Some(change)
        }
    }
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
    fn default_input_device_name_does_not_panic() {
        // Same reasoning as `input_device_names_does_not_panic`: we can't
        // assert on the actual value in a sandboxed/headless environment,
        // only that calling it is safe.
        let _ = default_input_device_name();
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

    #[test]
    fn is_bluetooth_name_matches_known_markers() {
        assert!(is_bluetooth_name("AirPods Pro"));
        assert!(is_bluetooth_name("Bluetooth Headset"));
        assert!(is_bluetooth_name("bluetooth headset")); // case-insensitive
        assert!(is_bluetooth_name("Jabra BT Speaker")); // whole-word "BT"
    }

    #[test]
    fn is_bluetooth_name_does_not_match_ordinary_devices() {
        assert!(!is_bluetooth_name("Built-in Microphone"));
        assert!(!is_bluetooth_name("USB Headset"));
        // "bt" appears as a plain substring here, but not as a whole word.
        assert!(!is_bluetooth_name("Subtotal Device"));
    }

    #[test]
    fn is_virtual_name_matches_known_markers() {
        assert!(is_virtual_name("BlackHole 2ch"));
        assert!(is_virtual_name("Soundflower (2ch)"));
        assert!(is_virtual_name("VB-Cable"));
        assert!(is_virtual_name("Loopback Audio"));
        assert!(is_virtual_name("Virtual Input"));
        assert!(is_virtual_name("Aggregate Device"));
    }

    #[test]
    fn is_virtual_name_does_not_match_ordinary_devices() {
        assert!(!is_virtual_name("Built-in Microphone"));
        assert!(!is_virtual_name("USB Headset"));
    }

    #[test]
    fn filter_input_devices_allows_everything_when_both_flags_true() {
        let names = vec![
            "Built-in Microphone".to_string(),
            "AirPods Pro".to_string(),
            "BlackHole 2ch".to_string(),
        ];
        let filtered = filter_input_devices(names.clone(), true, true);
        assert_eq!(filtered, names);
    }

    #[test]
    fn filter_input_devices_drops_bluetooth_when_disallowed() {
        let names = vec!["Built-in Microphone".to_string(), "AirPods Pro".to_string()];
        let filtered = filter_input_devices(names, false, true);
        assert_eq!(filtered, vec!["Built-in Microphone".to_string()]);
    }

    #[test]
    fn filter_input_devices_drops_virtual_when_disallowed() {
        let names = vec![
            "Built-in Microphone".to_string(),
            "BlackHole 2ch".to_string(),
        ];
        let filtered = filter_input_devices(names, true, false);
        assert_eq!(filtered, vec!["Built-in Microphone".to_string()]);
    }

    #[test]
    fn filter_input_devices_drops_both_when_both_disallowed() {
        let names = vec![
            "Built-in Microphone".to_string(),
            "AirPods Pro".to_string(),
            "BlackHole 2ch".to_string(),
        ];
        let filtered = filter_input_devices(names, false, false);
        assert_eq!(filtered, vec!["Built-in Microphone".to_string()]);
    }
}
