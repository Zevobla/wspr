//! The ASR section of the Models screen: ONE selector listing every local
//! whisper model on disk *and* the cloud ASR backends (no local/online
//! toggle -- see `crate::model_menu`), over a management table of the curated
//! whisper catalog (download / delete) plus any stray local whisper files.

use iced::widget::{column, pick_list};
use iced::Element;

use crate::hub::common::{field, section};
use crate::model_menu::{asr_options, selected_asr};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

use super::common::{body_text, catalog_table, delete_button, download_button, CatalogRow};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let selector = pick_list(
        asr_options(&state.hf_models, &state.config),
        selected_asr(&state.config),
        Message::HfAsrSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let mut rows: Vec<CatalogRow<'a>> = Vec::new();

    // The curated whisper catalog: download the ones you don't have, delete
    // the ones you do.
    for model in whspr_hf::MODELS {
        let installed = state
            .hf_models
            .asr
            .iter()
            .find(|m| m.filename == model.filename);
        let action = match installed {
            Some(m) => delete_button(scheme, state.hf_busy, m.path.clone()),
            None => download_button(scheme, state.hf_busy, Message::HfDownloadModel(model.id)),
        };
        rows.push(CatalogRow {
            name: model.label.to_string(),
            size_bytes: model.size_bytes,
            fit: state
                .hf_specs
                .fit(model.size_bytes, whspr_hf::ModelKind::Asr),
            action,
        });
    }

    // Whisper files the user dropped in themselves (not in the catalog):
    // deletable, and already selectable in the picker above.
    for m in state.hf_models.asr.iter().filter(|m| m.known_id.is_none()) {
        rows.push(CatalogRow {
            name: m.filename.clone(),
            size_bytes: m.size_bytes,
            fit: state.hf_specs.fit(m.size_bytes, whspr_hf::ModelKind::Asr),
            action: delete_button(scheme, state.hf_busy, m.path.clone()),
        });
    }

    section(
        scheme,
        "Speech recognition (ASR)",
        column![
            field(scheme, "Active model", selector.into()),
            body_text(
                "Downloaded models and cloud backends both appear in the selector above."
                    .to_string(),
                scheme,
            ),
            catalog_table(scheme, rows),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
