//! The Speakers screen: diarize a recording, pick the embedding model, and
//! rename enrolled speaker profiles. The roster renders as a Modernist
//! `table` (2px header rule, 1px row hairlines, flush-left) matching the
//! Models catalog -- see `hub::models::asr` for the sibling pattern.

use iced::widget::{button, column, pick_list, text, text_input};
use iced::{Element, Length};

use crate::config_ui;
use crate::state::{Message, State};
use crate::theme::widgets::{self, TagKind};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::{field, section};

/// Renders the Speakers screen.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    speakers_section(state, scheme)
}

fn speakers_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let embedding_picker = pick_list(
        config_ui::EMBEDDING_LABELS,
        Some(config_ui::embedding_label(
            state.config.speaker.embedding_model,
        )),
        Message::EmbeddingModelSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let pick_button = button(
        text("Diarize a recording...")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::filled(scheme, status))
    .on_press(Message::PickRecordingToDiarize);

    let mut body = column![
        field(scheme, "Embedding model", embedding_picker.into()),
        pick_button,
    ]
    .spacing(spacing::MD);

    if let Some(status) = &state.diarize_status {
        body = body.push(body_text(status.clone(), scheme));
    }

    body = body.push(speakers_roster(state, scheme));

    section(scheme, "Speakers", body.into())
}

/// The enrolled-speaker roster: a Modernist table, or a clean empty-state
/// line when nothing's been enrolled yet.
fn speakers_roster<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    if state.speaker_db.profiles.is_empty() {
        return body_text(
            "No speakers enrolled yet -- diarize a recording to populate this list.".to_string(),
            scheme,
        );
    }

    let rows: Vec<Vec<Element<'a, Message>>> = state
        .speaker_db
        .profiles
        .iter()
        .map(|profile| speaker_cells(profile, state, scheme))
        .collect();

    widgets::table(
        vec![
            ("Speaker", Length::Fill),
            ("Appearances", Length::Fixed(140.0)),
            ("Rename", Length::Fixed(200.0)),
            ("", Length::Fixed(96.0)),
        ],
        rows,
        scheme,
    )
}

/// One speaker's table row: its current display name, an appearance-count
/// tag, an inline rename `text_input`, and a Save action.
fn speaker_cells<'a>(
    profile: &whspr_config::SpeakerProfile,
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Vec<Element<'a, Message>> {
    let draft = state
        .speaker_rename_drafts
        .get(&profile.id)
        .cloned()
        .unwrap_or_else(|| profile.name.clone().unwrap_or_else(|| profile.id.clone()));
    let id_for_input = profile.id.clone();
    let id_for_submit = profile.id.clone();
    let can_save = !draft.trim().is_empty();

    vec![
        text(profile.name.clone().unwrap_or_else(|| profile.id.clone()))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface)
            .into(),
        widgets::tag(
            TagKind::Neutral,
            format!("{} scan(s)", profile.scans.len()),
            scheme,
        ),
        text_input("Speaker name", &draft)
            .size(type_scale::BODY_MEDIUM.size)
            .style(move |_theme, status| styles::text_input::outlined(scheme, status))
            .on_input(move |s| Message::SpeakerRenameInputChanged(id_for_input.clone(), s))
            .into(),
        button(
            text("Save")
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font()),
        )
        .style(move |_theme, status| styles::button::outlined(scheme, status))
        .on_press_maybe(can_save.then_some(Message::SpeakerRenameSubmitted(id_for_submit)))
        .into(),
    ]
}

/// A `BODY_MEDIUM`, de-emphasized paragraph -- the status/help wording used
/// across this screen.
fn body_text(content: String, scheme: &'static color::Scheme) -> Element<'static, Message> {
    text(content)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}
