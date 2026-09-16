//! Focused-field detection for `[capture].input_field_detection`: whether the
//! UI element that has keyboard focus right now accepts typed text, so a
//! dictation with nowhere sensible to go can land on the clipboard instead of
//! being typed into a button, a file list or a menu.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::focused_element_traits;

/// Off macOS there is no Accessibility query wired up yet, so the focused
/// element is never known.
#[cfg(not(target_os = "macos"))]
fn focused_element_traits() -> Option<(Option<String>, Option<bool>)> {
    None
}

/// Accessibility roles of text-entry controls.
const TEXT_INPUT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// Accessibility roles of focusable controls that never take typed text.
const NON_TEXT_ROLES: &[&str] = &[
    "AXButton",
    "AXCheckBox",
    "AXRadioButton",
    "AXPopUpButton",
    "AXMenuButton",
    "AXDisclosureTriangle",
    "AXSlider",
    "AXIncrementor",
    "AXColorWell",
    "AXImage",
    "AXStaticText",
    "AXLink",
    "AXList",
    "AXTable",
    "AXOutline",
    "AXBrowser",
    "AXRow",
    "AXCell",
    "AXMenu",
    "AXMenuItem",
    "AXMenuBarItem",
    "AXTabGroup",
    "AXToolbar",
    "AXProgressIndicator",
];

/// Whether the UI element with keyboard focus accepts typed text.
///
/// `Some(true)` for a text-entry control, `Some(false)` for a focusable
/// control that never takes text (a button, list, table, menu, ...), and
/// `None` when it cannot be determined: no Accessibility permission, an
/// Accessibility error, an element whose role says nothing either way (see
/// [`classify_focused_element`]), or a platform other than macOS. Callers
/// should keep their normal delivery on `None`.
///
/// This is a cross-process Accessibility query (bounded by a short
/// messaging timeout), so call it off the UI thread.
pub fn focused_field_is_editable() -> Option<bool> {
    let (role, value_settable) = focused_element_traits()?;
    classify_focused_element(role.as_deref(), value_settable)
}

/// Classifies the focused element from its Accessibility `role` and whether
/// its `AXValue` is settable (`value_settable`, `None` if that query failed).
///
/// - A text-entry role is editable.
/// - A known non-text control is not, even when its value is settable (a
///   slider's value is, but typing into it is never what the user meant).
/// - Otherwise a settable value marks a custom text view as editable.
/// - Anything else -- a web area or group whose app exposes no finer
///   accessibility tree, a custom-drawn terminal or editor, no role at all
///   -- is undeterminable (`None`), so callers keep typing as usual rather
///   than second-guess an app they cannot see into.
fn classify_focused_element(role: Option<&str>, value_settable: Option<bool>) -> Option<bool> {
    match role {
        Some(role) if TEXT_INPUT_ROLES.contains(&role) => Some(true),
        Some(role) if NON_TEXT_ROLES.contains(&role) => Some(false),
        _ if value_settable == Some(true) => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_elements_classify_by_role_then_settable_value() {
        // (role, value settable) -> editable?
        let cases = [
            ((Some("AXTextField"), Some(true)), Some(true)),
            ((Some("AXTextArea"), Some(false)), Some(true)),
            ((Some("AXComboBox"), None), Some(true)),
            ((Some("AXSearchField"), Some(true)), Some(true)),
            ((Some("AXButton"), Some(false)), Some(false)),
            ((Some("AXSlider"), Some(true)), Some(false)),
            ((Some("AXOutline"), None), Some(false)),
            ((Some("AXGroup"), Some(true)), Some(true)),
            ((Some("AXWebArea"), Some(false)), None),
            ((None, None), None),
        ];
        for ((role, settable), expected) in cases {
            assert_eq!(
                classify_focused_element(role, settable),
                expected,
                "role={role:?} settable={settable:?}"
            );
        }
    }

    /// A read-only query of whatever has focus while the tests run: without
    /// Accessibility permission (the usual case for a test binary) it is `None`,
    /// with it any answer is fine -- only a panic or crash in the FFI path
    /// would be a bug.
    #[test]
    fn focused_field_is_editable_does_not_panic() {
        let _ = focused_field_is_editable();
    }
}
