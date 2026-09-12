//! The Hub's navigation enums: which top-level screen the nav rail shows
//! (`Screen`) and which Settings sub-nav section is selected
//! (`SettingsSection`). Split out of `crate::state` so that file stays under
//! the AA-06 line cap; re-exported from `crate::state` so existing
//! `crate::state::Screen` / `SettingsSection` paths keep resolving.

/// Which top-level screen the Hub's left numbered nav rail (`crate::hub`)
/// is currently showing. `Dictate` is the default: whspr's core action
/// (record/dictate) gets the screen a user lands on. The declaration order
/// is the rail order (01 Dictate .. 05 Settings) -- see `crate::hub`'s
/// module doc for the redesign this drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Dictate,
    History,
    Models,
    Speakers,
    Settings,
}

/// Which section of the Settings screen's middle sub-nav is selected (the
/// Modernist rail -> sub-nav -> form three-column layout, mockup 1c). One
/// variant per real `Config` group (each maps 1:1 to a `hub::settings`
/// section view); declaration order is the sub-nav order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    General,
    Audio,
    Capture,
    Cleanup,
    Typing,
    Privacy,
    Accounts,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_default_is_dictate() {
        assert_eq!(Screen::default(), Screen::Dictate);
    }
}
