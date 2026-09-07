//! The Settings screen: the `Config` struct's editable fields, grouped into
//! one `common::section` "card" (or small cluster of cards) per sub-module
//! -- `general` (backend pickers, launch-at-login, sound), `capture`,
//! `injection`, `privacy`, `devices` (microphone/hotkey preview),
//! `normalize`, and `api_keys`.
//!
//! Split out of a single flat `settings.rs` once the file grew past this
//! project's 600-line-per-file guideline (AA-06); each group's controls,
//! `Message` wiring, and label helpers stay in its own file so no one file
//! has to hold the whole screen. `normalize.macros`/`normalize.dictionary`
//! are the one part of `Config` intentionally left unexposed here -- see
//! `normalize`'s module doc comment for why.

use iced::widget::column;
use iced::Element;

use crate::state::{Message, State};
use crate::theme::{color, spacing};

mod api_keys;
mod capture;
mod devices;
mod general;
mod injection;
mod normalize;
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
        normalize::view(state, scheme),
        api_keys::view(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}
