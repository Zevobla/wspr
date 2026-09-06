//! The Speakers screen: diarize a recording, pick the embedding model, and
//! rename enrolled speaker profiles.

use iced::widget::{button, column, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use crate::config_ui;
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::{field, section};

/// Renders the Speakers screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    speakers_section(state, scheme)
}

fn speakers_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let pick_button = button(
        text("Diarize a recording...")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::filled(scheme, status))
    .on_press(Message::PickRecordingToDiarize);

    let embedding_picker = pick_list(
        config_ui::EMBEDDING_LABELS,
        Some(config_ui::embedding_label(
            state.config.speaker.embedding_model,
        )),
        Message::EmbeddingModelSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let status: Element<'_, Message> = match &state.diarize_status {
        Some(status) => text(status.clone())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => column![].into(),
    };

    let profiles: Element<'_, Message> = if state.speaker_db.profiles.is_empty() {
        text("No speakers enrolled yet -- diarize a recording to populate this list.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into()
    } else {
        let rows: Vec<Element<'_, Message>> = state
            .speaker_db
            .profiles
            .iter()
            .map(|profile| speaker_row(profile, state, scheme))
            .collect();

        scrollable(column(rows).spacing(spacing::MD))
            .height(Length::Fixed(160.0))
            .style(move |_theme, status| styles::scrollable::rail(scheme, status))
            .into()
    };

    section(
        scheme,
        "Speakers",
        column![
            field(scheme, "Embedding model", embedding_picker.into()),
            pick_button,
            status,
            profiles,
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

fn speaker_row<'a>(
    profile: &whspr_config::SpeakerProfile,
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let draft = state
        .speaker_rename_drafts
        .get(&profile.id)
        .cloned()
        .unwrap_or_else(|| profile.name.clone().unwrap_or_else(|| profile.id.clone()));
    let id_for_input = profile.id.clone();
    let id_for_submit = profile.id.clone();
    let can_save = !draft.trim().is_empty();

    row![
        text(profile.name.clone().unwrap_or_else(|| profile.id.clone()))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface)
            .width(Length::Fixed(140.0)),
        text_input("Speaker name", &draft)
            .size(type_scale::BODY_MEDIUM.size)
            .style(move |_theme, status| styles::text_input::outlined(scheme, status))
            .on_input(move |s| Message::SpeakerRenameInputChanged(id_for_input.clone(), s))
            .width(Length::Fixed(180.0)),
        button(
            text("Save")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font())
        )
        .style(move |_theme, status| styles::button::outlined(scheme, status))
        .on_press_maybe(can_save.then_some(Message::SpeakerRenameSubmitted(id_for_submit))),
        text(format!("{} scan(s)", profile.scans.len()))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}
