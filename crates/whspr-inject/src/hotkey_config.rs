//! Turning a user-configurable hotkey *label* into a registerable combo.
//!
//! The listener no longer hardcodes `Ctrl+Space`: it registers whatever combo
//! `whspr-config` persisted (falling back to the platform default). A combo is
//! carried across crate boundaries as a plain string label — the exact format
//! [`crate::default_hotkey_label`] emits and the Hub's capture UI produces —
//! so neither `whspr-config` nor `whspr-app` has to name `global-hotkey`'s
//! `HotKey`/`Code` types. Parsing reuses `global-hotkey`'s own `FromStr`
//! (`"Ctrl+Shift+D"` → `HotKey`), so the accepted key set can never drift from
//! what the OS can actually register.

use global_hotkey::hotkey::{Code, HotKey};

use whspr_core::{Result, WhsprError};

use crate::{default_hotkey_modifiers, GlobalHotkeyListener};

/// The platform default hotkey as a registerable [`HotKey`] — plain
/// `Ctrl+Space`, or `Ctrl+Shift+Space` on Windows (see
/// [`default_hotkey_modifiers`]). Used when the user hasn't chosen a combo.
pub(crate) fn default_hotkey() -> HotKey {
    HotKey::new(Some(default_hotkey_modifiers()), Code::Space)
}

/// Parses a combo label (`"Ctrl+Shift+D"`) into a registerable [`HotKey`],
/// reusing `global-hotkey`'s own parser. Returns a `WhsprError::Inject` when
/// the label names a key/modifier the OS layer can't register, so a bad
/// persisted combo surfaces honestly instead of silently doing nothing.
pub(crate) fn parse_hotkey_label(label: &str) -> Result<HotKey> {
    label
        .parse::<HotKey>()
        .map_err(|e| WhsprError::Inject(format!("unsupported hotkey \"{label}\": {e}")))
}

/// Whether `label` is a combo the hotkey listener can actually register. Pure
/// (no OS calls), so the Hub's capture UI can reject an unusable key *before*
/// persisting it, rather than saving a combo that silently never binds. Note
/// this accepts a bare key like `"D"`; requiring a modifier is the caller's
/// policy, not the injector's.
pub fn hotkey_supported(label: &str) -> bool {
    label.parse::<HotKey>().is_ok()
}

impl GlobalHotkeyListener {
    /// Builds a listener that registers the combo described by `label`
    /// (e.g. `"Ctrl+Shift+D"`) instead of the platform default. Errors if the
    /// label can't be parsed into a registerable combo.
    pub fn from_label(label: &str) -> Result<Self> {
        Self::with_hotkey(parse_hotkey_label(label)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_hotkey_label;

    #[test]
    fn parse_hotkey_label_matches_the_platform_default() {
        // The default label must parse to exactly the combo `new()` registers,
        // so the UI hint, the persisted label, and the registered hotkey agree.
        assert_eq!(
            parse_hotkey_label(default_hotkey_label()).expect("default label must parse"),
            default_hotkey()
        );
    }

    #[test]
    fn hotkey_supported_accepts_a_modifier_combo() {
        assert!(hotkey_supported("Ctrl+Shift+D"));
        assert!(hotkey_supported("Cmd+Space"));
    }

    #[test]
    fn hotkey_supported_rejects_a_bare_modifier() {
        // A lone modifier is not a registerable hotkey (no main key).
        assert!(!hotkey_supported("Ctrl"));
        assert!(!hotkey_supported("Ctrl+Shift"));
    }

    #[test]
    fn hotkey_supported_rejects_gibberish() {
        assert!(!hotkey_supported("NotAKey"));
        assert!(!hotkey_supported(""));
    }

    #[test]
    fn parse_hotkey_label_errors_on_an_unsupported_combo() {
        let err = parse_hotkey_label("Ctrl+NotAKey").expect_err("should reject");
        assert!(matches!(err, WhsprError::Inject(_)));
    }
}
