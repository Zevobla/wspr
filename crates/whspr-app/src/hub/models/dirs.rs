//! The "Model directories" section of the Models screen: the user-managed
//! list of folders scanned for models (see `crate::hf::effective_model_dirs`),
//! with add (native folder picker) and per-directory remove actions. The
//! default models directory is always scanned and shown, but not removable.

use std::path::PathBuf;

use iced::widget::{button, column, row, text, Space};
use iced::{Alignment, Element, Length};

use crate::hub::common::section;
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

use super::common::{body_text, label_text};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();

    // The default download/scan directory: always scanned, so not removable.
    if let Some(default) = crate::hf::models_dir(&state.config) {
        rows.push(dir_row(scheme, default.display().to_string(), None));
    }

    // The user-added directories, each removable.
    for dir in &state.config.huggingface.model_dirs {
        rows.push(dir_row(
            scheme,
            dir.display().to_string(),
            Some(dir.clone()),
        ));
    }

    let add = button(label_text("Add directory"))
        .style(move |_theme, status| styles::button::filled(scheme, status))
        .on_press_maybe((!state.hf_busy).then_some(Message::HfAddModelDir));

    section(
        scheme,
        "Model directories",
        column![
            body_text(
                "whspr scans these folders for whisper and GGUF models. The default is always \
                 scanned; add more to surface models kept elsewhere."
                    .to_string(),
                scheme,
            ),
            column(rows).spacing(spacing::SM),
            add,
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

/// One directory row: the path, and either a "Remove" button (user-added) or
/// a muted "default" tag (the always-scanned default directory).
fn dir_row<'a>(
    scheme: &'static color::Scheme,
    path: String,
    removable: Option<PathBuf>,
) -> Element<'a, Message> {
    let path = text(path)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface)
        .width(Length::Fill);

    let trailing: Element<'a, Message> = match removable {
        Some(dir) => button(label_text("Remove"))
            .style(move |_theme, status| styles::button::outlined(scheme, status))
            .on_press(Message::HfRemoveModelDir(dir))
            .into(),
        None => text("default")
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(scheme.on_surface_variant)
            .into(),
    };

    row![path, Space::new().width(Length::Fixed(spacing::SM)), trailing]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
}
