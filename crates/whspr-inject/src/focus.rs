//! Focused-field detection for `[capture].input_field_detection`: whether the
//! UI element that has keyboard focus right now accepts typed text, so a
//! dictation with nowhere sensible to go can land on the clipboard instead of
//! being typed into a button, a file list or a menu.

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
