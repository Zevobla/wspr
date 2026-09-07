//! The Settings screen: the `Config` struct's editable fields, grouped into
//! one `common::section` "card" (or small cluster of cards) per sub-module
//! -- `general` (backend pickers, launch-at-login, sound), `capture`,
//! `injection`, `privacy`, and `devices` (microphone/hotkey preview) today,
//! with further groups added alongside this doc comment as more of
//! `Config` gets exposed here instead of requiring a hand-edited
//! `config.toml`.
//!
//! Split out of a single flat `settings.rs` once the file grew past this
//! project's 600-line-per-file guideline (AA-06); each group's controls,
//! `Message` wiring, and label helpers stay in its own file so no one file
//! has to hold the whole screen.

use iced::widget::column;
use iced::Element;

use crate::state::{Message, State};
use crate::theme::{color, spacing};

mod capture;
mod devices;
mod general;
mod injection;
mod privacy;

/// Renders the Settings screen: one `common::section` card per group,
/// stacked in the order they appear in the tab.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        general::view(state, scheme),
        capture::view(state, scheme),
        injection::view(state, scheme),
        privacy::view(state, scheme),
        devices::view(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}
