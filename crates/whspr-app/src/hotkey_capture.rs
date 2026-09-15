//! Formatting and the capture decision for the Hub's push-to-talk rebind.
//!
//! `format_key_combo` turns a captured `iced` key press into a
//! `global-hotkey`-parseable label like `"Ctrl+Shift+D"`;
//! [`capture_outcome`] decides what a press means while capturing -- bind a
//! complete combo, keep waiting for one, or cancel on Escape. A bound combo
//! is persisted to `config.hotkey` and registered by the listener on the next
//! launch (see `crate::worker`), so a rebind actually takes effect rather than
//! being a dead preview.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// Formats a modifiers + key combination the way a hotkey display normally
/// looks, e.g. `"Ctrl+Shift+D"`. Named keys fall back to their `Debug` form
/// (e.g. `"Space"`, `"F5"`); character keys are upper-cased for consistency.
pub fn format_key_combo(modifiers: Modifiers, key: &Key) -> String {
    let mut parts = Vec::new();

    if modifiers.control() {
        parts.push("Ctrl".to_string());
    }
    if modifiers.alt() {
        parts.push("Alt".to_string());
    }
    if modifiers.shift() {
        parts.push("Shift".to_string());
    }
    if modifiers.logo() {
        parts.push("Cmd".to_string());
    }

    parts.push(match key {
        Key::Character(c) => c.to_uppercase(),
        Key::Named(named) => format!("{named:?}"),
        Key::Unidentified => "?".to_string(),
    });

    parts.join("+")
}

/// What a key press received while capturing a new push-to-talk hotkey means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// A complete, registerable combo: persist it and stop capturing. Carries
    /// the combo label (e.g. `"Ctrl+Shift+D"`).
    Bound(String),
    /// A bare modifier or an unusable key: keep listening for a real combo.
    Incomplete,
    /// Escape was pressed: cancel capture without changing the hotkey.
    Cancelled,
}

/// Decides what a captured press means. Pure, so the bind/keep-waiting/cancel
/// distinction is unit-testable without a real keyboard subscription (mirrors
/// `worker::capture_decision`). A combo is only [`Bound`](CaptureOutcome::Bound)
/// when it carries at least one modifier -- a bare key would hijack normal
/// typing system-wide -- and the injector can actually register it
/// (`whspr_inject::hotkey_supported`). Escape cancels; anything else (a lone
/// modifier mid-chord, an unsupported key) keeps capturing.
pub fn capture_outcome(modifiers: Modifiers, key: &Key) -> CaptureOutcome {
    if matches!(key, Key::Named(Named::Escape)) {
        return CaptureOutcome::Cancelled;
    }

    let combo = format_key_combo(modifiers, key);
    if combo.contains('+') && whspr_inject::hotkey_supported(&combo) {
        CaptureOutcome::Bound(combo)
    } else {
        CaptureOutcome::Incomplete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_plain_character_key() {
        assert_eq!(
            format_key_combo(Modifiers::default(), &Key::Character("d".into())),
            "D"
        );
    }

    #[test]
    fn formats_control_shift_character() {
        let modifiers = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            format_key_combo(modifiers, &Key::Character("d".into())),
            "Ctrl+Shift+D"
        );
    }

    #[test]
    fn formats_named_key() {
        assert_eq!(
            format_key_combo(Modifiers::default(), &Key::Named(Named::Space)),
            "Space"
        );
    }

    #[test]
    fn formats_unidentified_key() {
        assert_eq!(
            format_key_combo(Modifiers::default(), &Key::Unidentified),
            "?"
        );
    }

    #[test]
    fn capture_outcome_binds_a_modifier_combo() {
        let modifiers = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            capture_outcome(modifiers, &Key::Character("d".into())),
            CaptureOutcome::Bound("Ctrl+Shift+D".to_string())
        );
    }

    #[test]
    fn capture_outcome_ignores_a_bare_key() {
        // No modifier: binding it would hijack the plain "d" key everywhere.
        assert_eq!(
            capture_outcome(Modifiers::default(), &Key::Character("d".into())),
            CaptureOutcome::Incomplete
        );
    }

    #[test]
    fn capture_outcome_ignores_a_lone_modifier_press() {
        // Pressing Ctrl on its way to a chord isn't a registerable combo yet.
        assert_eq!(
            capture_outcome(Modifiers::CTRL, &Key::Named(Named::Control)),
            CaptureOutcome::Incomplete
        );
    }

    #[test]
    fn capture_outcome_cancels_on_escape() {
        assert_eq!(
            capture_outcome(Modifiers::default(), &Key::Named(Named::Escape)),
            CaptureOutcome::Cancelled
        );
    }
}
