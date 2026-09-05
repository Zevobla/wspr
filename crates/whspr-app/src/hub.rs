//! The Hub window: settings, device list, hotkey config, history, and stats.

use iced::widget::{column, container, pick_list, text, text_input};
use iced::{Element, Length};

use crate::config_ui::{self, ASR_LABELS, REFINE_LABELS};
use crate::state::{Message, State};

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    container(
        column![
            text("whspr").size(28),
            settings_section(state),
        ]
        .spacing(20)
        .padding(20)
        .width(Length::Fill),
    )
    .into()
}

fn settings_section(state: &State) -> Element<'_, Message> {
    let asr_picker = pick_list(
        ASR_LABELS,
        Some(config_ui::asr_label(state.config.asr)),
        Message::AsrSelected,
    );

    let refine_picker = pick_list(
        REFINE_LABELS,
        Some(config_ui::refine_label(state.config.refine)),
        Message::RefineSelected,
    );

    let language_input = text_input("e.g. en, es, fr (blank = auto-detect)", &state.language_input)
        .on_input(Message::LanguageChanged);

    column![
        text("Settings").size(20),
        column![text("ASR backend"), asr_picker].spacing(6),
        column![text("Refiner"), refine_picker].spacing(6),
        column![text("Language override"), language_input].spacing(6),
    ]
    .spacing(12)
    .into()
}
