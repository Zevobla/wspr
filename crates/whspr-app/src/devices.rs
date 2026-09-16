//! Input-device helpers shared by the live-dictation worker and the Settings
//! device picker.

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
