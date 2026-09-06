//! The Hub window: settings, device list, hotkey config, history, and
//! stats -- restyled on the MD3 tokens in `crate::theme` (see that
//! module's doc comment for the overall token system this app styles
//! from). Every section is an M3 "card" (`section`, tonal
//! `surface_container_low`); every field caption/control pair is grouped
//! with `field`.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input, Space,
};
use iced::{Alignment, Element, Length};

use crate::config_ui::{self, ASR_LABELS, REFINE_LABELS};
use crate::state::{Message, State};
use crate::stats;
use crate::theme::{self, color, spacing, styles, type_scale};

/// Renders the Hub window's content for the current state.
pub fn view(state: &State) -> Element<'_, Message> {
    let scheme = theme::scheme(&state.theme);

    let body = scrollable(
        column![
            settings_section(state, scheme),
            device_section(state, scheme),
            hotkey_section(state, scheme),
            speakers_section(state, scheme),
            history_section(state, scheme),
            stats_section(state, scheme),
        ]
        .spacing(spacing::XL),
    )
    .style(move |_theme, status| styles::scrollable::rail(scheme, status));

    container(
        column![
            header(state, scheme),
            divider(scheme),
            error_banner(state, scheme),
            body
        ]
        .spacing(spacing::LG)
        .padding(spacing::XL)
        .width(Length::Fill),
    )
    .style(move |_theme| styles::container::surface(scheme))
    .into()
}

/// A settings-panel "card": a title in `TITLE_MEDIUM` over `body`, tonal
/// background from `styles::container::section`.
fn section<'a>(
    scheme: &'static color::Scheme,
    title: &'static str,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .size(type_scale::TITLE_MEDIUM.size)
                .font(type_scale::TITLE_MEDIUM.font())
                .color(scheme.on_surface),
            body,
        ]
        .spacing(spacing::MD),
    )
    .padding(spacing::LG)
    .width(Length::Fill)
    .style(move |_theme| styles::container::section(scheme))
    .into()
}

/// A field caption (`LABEL_MEDIUM`, de-emphasized) tightly grouped above
/// its control -- MD3's "tight groups, generous separation" spacing rule.
fn field<'a>(
    scheme: &'static color::Scheme,
    label: &'static str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant),
        control,
    ]
    .spacing(spacing::XS)
    .into()
}

fn header<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let theme_label = match state.theme {
        iced::Theme::Dark => "Switch to light",
        _ => "Switch to dark",
    };

    row![
        text("whspr")
            .size(type_scale::TITLE_LARGE.size)
            .font(type_scale::TITLE_LARGE.font())
            .color(scheme.on_surface)
            .width(Length::Fill),
        button(
            text(theme_label)
                .size(type_scale::LABEL_LARGE.size)
                .font(type_scale::LABEL_LARGE.font())
        )
        .style(move |_theme, status| styles::button::text(scheme, status))
        .on_press(Message::ThemeToggled),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A 1px `outline_variant` rule under the header.
fn divider(scheme: &'static color::Scheme) -> Element<'static, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme| styles::container::divider(scheme))
        .into()
}

fn error_banner<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    match &state.last_error {
        Some(error) => container(
            text(format!("Last worker error: {error}"))
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font()),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| styles::container::error_banner(scheme))
        .into(),
        None => column![].into(),
    }
}

fn device_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let device_picker = pick_list(
        state.input_devices.clone(),
        state.selected_device.clone(),
        Message::DeviceSelected,
    )
    .placeholder("No input devices found")
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    section(
        scheme,
        "Microphone",
        field(scheme, "Input device", device_picker.into()),
    )
}

fn hotkey_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let capture_label = if state.hotkey_capturing {
        "Press any key..."
    } else {
        "Preview a new hotkey"
    };
    let capture_button = button(
        text(capture_label)
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::tonal(scheme, status))
    .on_press(Message::StartHotkeyCapture);

    let preview: Element<'_, Message> = match &state.captured_hotkey {
        Some(combo) => text(format!("Captured: {combo} (preview only, not yet applied)"))
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
        None => text("No preview captured yet")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    section(
        scheme,
        "Hotkey",
        column![
            text(
                "Active hotkey: Ctrl+Space (fixed -- whspr-inject doesn't yet support \
                  registering a different combo at runtime)"
            )
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant),
            capture_button,
            preview,
        ]
        .spacing(spacing::SM)
        .into(),
    )
}

fn history_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let content: Element<'_, Message> = if state.history.is_empty() {
        text("No transcriptions yet -- dictate something to see it here.")
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into()
    } else {
        // Most recent first.
        let entries: Vec<Element<'_, Message>> = state
            .history
            .iter()
            .rev()
            .map(|entry| {
                text(entry.text.clone())
                    .size(type_scale::BODY_MEDIUM.size)
                    .font(type_scale::BODY_MEDIUM.font())
                    .color(scheme.on_surface)
                    .into()
            })
            .collect();

        scrollable(column(entries).spacing(spacing::SM))
            .height(Length::Fixed(160.0))
            .style(move |_theme, status| styles::scrollable::rail(scheme, status))
            .into()
    };

    section(scheme, "History", content)
}

fn stats_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let line = |content: String| {
        text(content)
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::BODY_MEDIUM.font())
            .color(scheme.on_surface)
    };

    let count_line = line(format!(
        "Transcriptions this session: {}",
        state.history.len()
    ));

    let wpm_line = match stats::average_wpm(&state.history) {
        Some(wpm) => line(format!("Average speed: {wpm:.0} wpm")),
        None => line("Average speed: not enough data yet".to_string()),
    };

    section(
        scheme,
        "Stats",
        column![count_line, wpm_line].spacing(spacing::XS).into(),
    )
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

fn settings_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let asr_picker = pick_list(
        ASR_LABELS,
        Some(config_ui::asr_label(state.config.asr)),
        Message::AsrSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let refine_picker = pick_list(
        REFINE_LABELS,
        Some(config_ui::refine_label(state.config.refine)),
        Message::RefineSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let language_picker = pick_list(
        config_ui::LANGUAGE_LABELS,
        Some(config_ui::language_label(&state.config.language)),
        |label: &'static str| Message::LanguageChanged(label.to_string()),
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let autostart_toggle: Element<'_, Message> = checkbox(state.config.autostart.enabled)
        .label("Launch at login")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::AutostartToggled)
        .into();

    section(
        scheme,
        "Settings",
        column![
            field(scheme, "ASR backend", asr_picker.into()),
            field(scheme, "Refiner", refine_picker.into()),
            field(
                scheme,
                "Language ('auto' = no override)",
                language_picker.into()
            ),
            autostart_toggle,
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
