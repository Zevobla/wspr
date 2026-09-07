//! The "Normalize" (Cleanup) section: whspr-refine's rule-based
//! text-normalization toggles (numbers/dates/times, paragraph breaks,
//! punctuation) plus the number-rendering choice as a segmented control.
//!
//! `normalize.macros`/`normalize.dictionary` (`BTreeMap<String, String>`
//! lookup tables) are deliberately not editable here -- a usable add/edit/
//! remove list UI for a string-keyed map is a bigger chunk of work than the
//! rest of this section combined, so it's left for a follow-up. Both still
//! round-trip through the config file untouched.

use iced::widget::column;
use iced::Element;

use crate::config_ui::{self, NUMBER_FORMAT_LABELS};
use crate::hub::common::{field, section, toggle_row};
use crate::state::{Message, State};
use crate::theme::widgets;
use crate::theme::{color, spacing};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let current = config_ui::number_format_label(state.config.normalize.numbers_format);
    let options: Vec<(String, bool)> = NUMBER_FORMAT_LABELS
        .iter()
        .map(|label| ((*label).to_string(), *label == current))
        .collect();
    let number_format = widgets::segmented(
        options,
        |i| Message::NumberFormatSelected(NUMBER_FORMAT_LABELS[i]),
        scheme,
    );

    section(
        scheme,
        "Cleanup",
        column![
            toggle_row(
                scheme,
                "Normalize spoken numbers to digits",
                state.config.normalize.numbers,
                Message::NormalizeNumbersToggled,
            ),
            toggle_row(
                scheme,
                "Normalize dates to YYYY-MM-DD",
                state.config.normalize.dates,
                Message::NormalizeDatesToggled,
            ),
            toggle_row(
                scheme,
                "Normalize times to 24-hour HH:MM",
                state.config.normalize.times,
                Message::NormalizeTimesToggled,
            ),
            field(scheme, "Number rendering", number_format),
            toggle_row(
                scheme,
                "Insert paragraph breaks on long pauses",
                state.config.normalize.paragraph_break,
                Message::ParagraphBreakToggled,
            ),
            toggle_row(
                scheme,
                "Auto-punctuate",
                state.config.normalize.punctuation_toggle,
                Message::PunctuationToggleToggled,
            ),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
