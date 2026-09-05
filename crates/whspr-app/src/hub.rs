//! The Hub window: settings, device list, hotkey config, history, and stats.

use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Element, Length, Theme};

use crate::config_ui::{self, ASR_LABELS, REFINE_LABELS};
use crate::state::{Message, State};
use crate::stats;

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    container(
        column![
            header(state),
            error_banner(state),
            settings_section(state),
            device_section(state),
            hotkey_section(state),
            history_section(state),
            stats_section(state),
            speakers_section(state),
        ]
        .spacing(20)
        .padding(20)
        .width(Length::Fill),
    )
    .into()
}

fn device_section(state: &State) -> Element<'_, Message> {
    let device_picker = pick_list(
        state.input_devices.clone(),
        state.selected_device.clone(),
        Message::DeviceSelected,
    )
    .placeholder("No input devices found");

    column![
        text("Microphone").size(20),
        column![text("Input device"), device_picker].spacing(6),
    ]
    .spacing(12)
    .into()
}

fn hotkey_section(state: &State) -> Element<'_, Message> {
    let capture_label = if state.hotkey_capturing {
        "Press any key..."
    } else {
        "Preview a new hotkey"
    };
    let capture_button = button(text(capture_label)).on_press(Message::StartHotkeyCapture);

    let preview: Element<'_, Message> = match &state.captured_hotkey {
        Some(combo) => text(format!("Captured: {combo} (preview only, not yet applied)")).into(),
        None => text("No preview captured yet").into(),
    };

    column![
        text("Hotkey").size(20),
        text(
            "Active hotkey: Ctrl+Space (fixed -- whspr-inject doesn't yet support \
              registering a different combo at runtime)"
        ),
        capture_button,
        preview,
    ]
    .spacing(8)
    .into()
}

fn header(state: &State) -> Element<'_, Message> {
    let theme_label = match state.theme {
        Theme::Dark => "Switch to light",
        _ => "Switch to dark",
    };

    row![
        text("whspr").size(28).width(Length::Fill),
        button(text(theme_label)).on_press(Message::ThemeToggled),
    ]
    .into()
}

fn error_banner(state: &State) -> Element<'_, Message> {
    match &state.last_error {
        Some(error) => text(format!("Last worker error: {error}")).into(),
        None => column![].into(),
    }
}

fn history_section(state: &State) -> Element<'_, Message> {
    let content: Element<'_, Message> = if state.history.is_empty() {
        text("No transcriptions yet -- dictate something to see it here.").into()
    } else {
        // Most recent first.
        let entries: Vec<Element<'_, Message>> = state
            .history
            .iter()
            .rev()
            .map(|entry| text(entry.text.clone()).into())
            .collect();

        scrollable(column(entries).spacing(6))
            .height(Length::Fixed(160.0))
            .into()
    };

    column![text("History").size(20), content].spacing(8).into()
}

fn stats_section(state: &State) -> Element<'_, Message> {
    let count_line = text(format!(
        "Transcriptions this session: {}",
        state.history.len()
    ));

    let wpm_line = match stats::average_wpm(&state.history) {
        Some(wpm) => text(format!("Average speed: {wpm:.0} wpm")),
        None => text("Average speed: not enough data yet"),
    };

    column![text("Stats").size(20), count_line, wpm_line]
        .spacing(6)
        .into()
}

fn speakers_section(state: &State) -> Element<'_, Message> {
    let pick_button =
        button(text("Diarize a recording...")).on_press(Message::PickRecordingToDiarize);

    let status: Element<'_, Message> = match &state.diarize_status {
        Some(status) => text(status.clone()).into(),
        None => column![].into(),
    };

    let profiles: Element<'_, Message> = if state.speaker_db.profiles.is_empty() {
        text("No speakers enrolled yet -- diarize a recording to populate this list.").into()
    } else {
        let rows: Vec<Element<'_, Message>> = state
            .speaker_db
            .profiles
            .iter()
            .map(|profile| {
                let draft = state
                    .speaker_rename_drafts
                    .get(&profile.id)
                    .cloned()
                    .unwrap_or_else(|| profile.name.clone().unwrap_or_else(|| profile.id.clone()));
                let id_for_input = profile.id.clone();
                let id_for_submit = profile.id.clone();

                row![
                    text(profile.name.clone().unwrap_or_else(|| profile.id.clone()))
                        .width(Length::Fixed(140.0)),
                    text_input("Speaker name", &draft)
                        .on_input(move |s| {
                            Message::SpeakerRenameInputChanged(id_for_input.clone(), s)
                        })
                        .width(Length::Fixed(180.0)),
                    button(text("Save")).on_press(Message::SpeakerRenameSubmitted(id_for_submit)),
                    text(format!("{} scan(s)", profile.scans.len())),
                ]
                .spacing(10)
                .into()
            })
            .collect();

        scrollable(column(rows).spacing(8))
            .height(Length::Fixed(160.0))
            .into()
    };

    column![text("Speakers").size(20), pick_button, status, profiles]
        .spacing(8)
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

    let language_input = text_input(
        "e.g. en, es, fr (blank = auto-detect)",
        &state.language_input,
    )
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
