//! The Settings screen: the Modernist rail -> sub-nav -> form layout. The
//! rail is the Hub shell's; this screen supplies the middle sub-nav column
//! (one entry per `SettingsSection`) and, to its right, the selected
//! section's form.
//!
//! Each section maps 1:1 to a `Config` group and its own sub-module view
//! (`general`, `devices`, `capture`, `normalize`, `injection`, `privacy`,
//! `api_keys`), each rendered as a flush `common::section` block. Splitting
//! the config surface further into the mockup's eleven labels (Macros,
//! Dictionary, ...) would need a control-level refactor beyond this
//! restyle; the seven real groups are the honest sub-nav (see the report).

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::state::{Message, SettingsSection, State};
use crate::theme::widgets::{self, Mark};
use crate::theme::{color, spacing, type_scale};

mod api_keys;
mod capture;
mod devices;
mod general;
mod injection;
mod normalize;
mod privacy;
mod update;

pub(crate) use update::update;

/// The sub-nav entries, in order.
const SECTIONS: [(SettingsSection, &str); 7] = [
    (SettingsSection::General, "General"),
    (SettingsSection::Audio, "Audio & devices"),
    (SettingsSection::Capture, "Capture"),
    (SettingsSection::Cleanup, "Cleanup"),
    (SettingsSection::Typing, "Typing"),
    (SettingsSection::Privacy, "Privacy"),
    (SettingsSection::Accounts, "Accounts & keys"),
];

/// Renders the Settings screen: sub-nav column, a 1px rule, then the
/// selected section's form.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let subnav = column(
        SECTIONS
            .into_iter()
            .map(|(section, label)| subnav_item(section, label, state.settings_section, scheme)),
    )
    .spacing(spacing::XS)
    .width(Length::Fixed(spacing::layout::SETTINGS_SUBNAV_W));

    let form = form_for(state, scheme);

    row![
        subnav,
        widgets::vrule(spacing::layout::HAIRLINE, scheme),
        container(form)
            .width(Length::Fill)
            .padding([0.0, spacing::XL]),
    ]
    .width(Length::Fill)
    .into()
}

/// Dispatches to the selected section's form view.
fn form_for<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match state.settings_section {
        SettingsSection::General => general::view(state, scheme),
        SettingsSection::Audio => devices::view(state, scheme),
        SettingsSection::Capture => capture::view(state, scheme),
        SettingsSection::Cleanup => normalize::view(state, scheme),
        SettingsSection::Typing => injection::view(state, scheme),
        SettingsSection::Privacy => privacy::view(state, scheme),
        SettingsSection::Accounts => api_keys::view(state, scheme),
    }
}

/// One sub-nav entry: a flush-left button; active gets accent text + a
/// trailing 8px accent square.
fn subnav_item<'a>(
    section: SettingsSection,
    label: &'static str,
    current: SettingsSection,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let active = section == current;
    let text_color = if active {
        scheme.primary
    } else {
        scheme.on_surface
    };
    let mark: Element<'a, Message> = if active {
        widgets::status_square(Mark::Solid, 8.0, scheme)
    } else {
        Space::new().width(Length::Fixed(8.0)).into()
    };

    button(
        row![
            text(label)
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font())
                .color(text_color)
                .width(Length::Fill),
            mark,
        ]
        .align_y(Alignment::Center)
        .spacing(spacing::SM),
    )
    .width(Length::Fill)
    .padding([8.0, 12.0])
    .on_press(Message::SettingsSectionSelected(section))
    .style(move |_theme, status| super::ghost_row(scheme, status))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnav_lists_every_section_once() {
        assert_eq!(SECTIONS.len(), 7);
        for (i, (a, _)) in SECTIONS.iter().enumerate() {
            for (b, _) in &SECTIONS[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn general_is_the_default_section() {
        assert_eq!(SettingsSection::default(), SettingsSection::General);
    }
}
