//! Pure formatting for the Hub's hotkey-capture preview.
//!
//! `whspr_inject::GlobalHotkeyListener` hardcodes Ctrl+Space and doesn't
//! expose a way to register a different combo at runtime (its
//! `GlobalHotKeyManager` is a private field with no accessor) -- see
//! `crate::hub`'s hotkey section for the full limitation writeup. This
//! module only turns a captured `iced` key press into a human-readable
//! string like `"Ctrl+Shift+D"`, independent of whether it can ever be
//! applied.

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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Named;

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
}
