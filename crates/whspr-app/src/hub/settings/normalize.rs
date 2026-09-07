//! The "Normalize" section: whspr-refine's rule-based text-normalization
//! toggles (numbers/dates/times, paragraph breaks, punctuation).
//!
//! `normalize.macros`/`normalize.dictionary` (`BTreeMap<String, String>`
//! lookup tables) are deliberately not editable here -- a usable add/edit/
//! remove list UI for a string-keyed map is a bigger chunk of work than the
//! rest of this section combined, so it's left for a follow-up rather than
//! bloating this pass. Both still round-trip through the config file
//! untouched; they just aren't reachable from the Hub yet.

use iced::widget::{checkbox, column, pick_list};
use iced::Element;

use crate::config_ui::{self, NUMBER_FORMAT_LABELS};
use crate::hub::common::{field, section};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let numbers = checkbox(state.config.normalize.numbers)
        .label("Normalize spoken numbers to digits")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::NormalizeNumbersToggled);

    let dates = checkbox(state.config.normalize.dates)
        .label("Normalize dates to YYYY-MM-DD")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::NormalizeDatesToggled);

    let times = checkbox(state.config.normalize.times)
        .label("Normalize times to 24-hour HH:MM")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::NormalizeTimesToggled);

    let paragraph_break = checkbox(state.config.normalize.paragraph_break)
        .label("Insert paragraph breaks on long pauses")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::ParagraphBreakToggled);

    let punctuation_toggle = checkbox(state.config.normalize.punctuation_toggle)
        .label("Auto-punctuate")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::PunctuationToggleToggled);

    let number_format_picker = pick_list(
        NUMBER_FORMAT_LABELS,
        Some(config_ui::number_format_label(
            state.config.normalize.numbers_format,
        )),
        Message::NumberFormatSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    section(
        scheme,
        "Normalize",
        column![
            numbers,
            dates,
            times,
            field(scheme, "Number rendering", number_format_picker.into()),
            paragraph_break,
            punctuation_toggle,
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
