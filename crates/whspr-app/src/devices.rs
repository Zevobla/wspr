//! Input-device helpers shared by the live-dictation worker and the Settings
//! device picker: the fallback notice, which devices the picker lists, and
//! the hotplug watcher that keeps that list current.

use whspr_audio::DeviceChange;

/// The notice shown when the configured input device `name` is not
/// connected, so recording falls back to the OS default input device. One
/// wording for every place that detects it -- the worker at capture start
/// and the Settings hotplug watcher -- so the user always sees the same
/// message.
pub(crate) fn fallback_notice(name: &str) -> String {
    format!(
        "\u{201c}{name}\u{201d} is not connected \u{2014} recording from the default microphone instead."
    )
}

/// What the Settings input-device picker lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DevicePicker {
    /// Connected devices allowed by the `[device]` Bluetooth/virtual source
    /// toggles, in host order -- plus the configured device at the end when
    /// the filter or a disconnect would otherwise hide it.
    pub options: Vec<String>,
    /// Why the configured device is listed anyway, when it is.
    pub note: Option<String>,
}

/// Builds the picker from the `connected` device names, the `configured`
/// `[device].input_device`, and the source toggles (see
/// `whspr_audio::filter_input_devices`). The configured device is never
/// silently dropped: a filtered-out or disconnected selection stays listed,
/// with a note saying which it is.
pub(crate) fn device_picker(
    connected: &[String],
    configured: Option<&str>,
    allow_bluetooth: bool,
    allow_virtual: bool,
) -> DevicePicker {
    let mut options =
        whspr_audio::filter_input_devices(connected.to_vec(), allow_bluetooth, allow_virtual);
    let note = match configured {
        Some(name) if !options.iter().any(|option| option == name) => {
            options.push(name.to_string());
            Some(if connected.iter().any(|device| device == name) {
                format!(
                    "\u{201c}{name}\u{201d} is hidden by the source settings above but is \
                     still the microphone whspr records from."
                )
            } else {
                fallback_notice(name)
            })
        }
        _ => None,
    };
    DevicePicker { options, note }
}

/// `connected` after a hotplug `change`: removed devices dropped, newly
/// added ones appended, nothing listed twice.
pub(crate) fn apply_device_change(connected: &[String], change: &DeviceChange) -> Vec<String> {
    let mut devices: Vec<String> = connected
        .iter()
        .filter(|device| !change.removed.contains(device))
        .cloned()
        .collect();
    for added in &change.added {
        if !devices.contains(added) {
            devices.push(added.clone());
        }
    }
    devices
}

/// How a hotplug change affects `State::notice` for the `configured` input
/// device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoticeUpdate {
    /// Leave the notice as it is.
    Keep,
    /// The configured device just disconnected: show this fallback notice.
    Show(String),
    /// The configured device came back while its fallback notice is still
    /// showing: clear it.
    Clear,
}

/// Decides [`NoticeUpdate`] for a hotplug `change`, given the notice
/// currently showing (`current`). Only the fallback notice for the
/// configured device is ever cleared -- an unrelated notice stays up.
pub(crate) fn hotplug_notice(
    configured: Option<&str>,
    change: &DeviceChange,
    current: Option<&str>,
) -> NoticeUpdate {
    let Some(name) = configured else {
        return NoticeUpdate::Keep;
    };
    if change.removed.iter().any(|removed| removed == name) {
        return NoticeUpdate::Show(fallback_notice(name));
    }
    let returned = change.added.iter().any(|added| added == name);
    if returned && current == Some(fallback_notice(name).as_str()) {
        return NoticeUpdate::Clear;
    }
    NoticeUpdate::Keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_notice_names_the_missing_device() {
        let notice = fallback_notice("USB Mic");
        assert!(notice.contains("USB Mic"));
        assert!(notice.contains("default microphone"));
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn picker_filters_bluetooth_and_virtual_sources_by_the_toggles() {
        let connected = names(&["Built-in Microphone", "AirPods Pro", "BlackHole 2ch"]);

        let all = device_picker(&connected, None, true, true);
        assert_eq!(all.options, connected);
        assert_eq!(all.note, None);

        let physical_only = device_picker(&connected, None, false, false);
        assert_eq!(physical_only.options, names(&["Built-in Microphone"]));
        assert_eq!(physical_only.note, None);
    }

    #[test]
    fn picker_keeps_a_filtered_out_selection_with_a_note() {
        let connected = names(&["Built-in Microphone", "AirPods Pro"]);
        let picker = device_picker(&connected, Some("AirPods Pro"), false, true);
        assert_eq!(
            picker.options,
            names(&["Built-in Microphone", "AirPods Pro"])
        );
        assert!(picker
            .note
            .unwrap()
            .contains("hidden by the source settings"));
    }

    #[test]
    fn picker_keeps_a_disconnected_selection_with_the_fallback_notice() {
        let connected = names(&["Built-in Microphone"]);
        let picker = device_picker(&connected, Some("USB Mic"), true, true);
        assert_eq!(picker.options, names(&["Built-in Microphone", "USB Mic"]));
        assert_eq!(picker.note, Some(fallback_notice("USB Mic")));
    }

    #[test]
    fn a_hotplug_change_drops_removed_and_appends_added_devices() {
        let change = DeviceChange {
            added: names(&["USB Mic", "Built-in Microphone"]),
            removed: names(&["AirPods Pro"]),
        };
        let updated = apply_device_change(&names(&["Built-in Microphone", "AirPods Pro"]), &change);
        assert_eq!(updated, names(&["Built-in Microphone", "USB Mic"]));
    }
}
