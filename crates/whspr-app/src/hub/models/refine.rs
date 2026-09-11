//! The refiner section of the Models screen: ONE selector listing None, the
//! cloud refiners (OpenAI/Anthropic), *and* every local GGUF LLM on disk (no
//! local/online toggle -- see `crate::model_menu`), over a management table
//! of the curated GGUF catalog (download / delete) plus any stray local GGUF.

use iced::widget::{column, pick_list};
use iced::Element;

use crate::hub::common::{field, section};
use crate::model_menu::{refine_options, selected_refine};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

use super::common::{body_text, catalog_table, delete_button, download_button, CatalogRow};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let selector = pick_list(
        refine_options(&state.hf_models, &state.config),
        selected_refine(&state.config),
        Message::HfRefineSelected,
    )
    .style(move |_theme, status| styles::pick_list::field(scheme, status))
    .menu_style(move |_theme| styles::pick_list::menu(scheme));

    let mut rows: Vec<CatalogRow<'a>> = Vec::new();

    // The curated GGUF refiner catalog.
    for model in whspr_hf::LLM_MODELS {
        let installed = state
            .hf_models
            .llm
            .iter()
            .find(|m| m.filename == model.filename);
        let action = match installed {
            Some(m) => delete_button(scheme, state.hf_busy, m.path.clone()),
            None => download_button(scheme, state.hf_busy, Message::HfDownloadLlm(model.id)),
        };
        rows.push(CatalogRow {
            name: model.label.to_string(),
            size_bytes: model.size_bytes,
            fit: state
                .hf_specs
                .fit(model.size_bytes, whspr_hf::ModelKind::Llm),
            action,
        });
    }

    // GGUF files the user dropped in themselves (not in the catalog).
    for m in state.hf_models.llm.iter().filter(|m| m.known_id.is_none()) {
        rows.push(CatalogRow {
            name: m.filename.clone(),
            size_bytes: m.size_bytes,
            fit: state.hf_specs.fit(m.size_bytes, whspr_hf::ModelKind::Llm),
            action: delete_button(scheme, state.hf_busy, m.path.clone()),
        });
    }

    section(
        scheme,
        "Text refinement (LLM)",
        column![
            field(scheme, "Active refiner", selector.into()),
            body_text(
                "Local GGUF LLMs and cloud refiners both appear in the selector above; \
                 \"None\" leaves the raw transcript untouched."
                    .to_string(),
                scheme,
            ),
            catalog_table(scheme, rows),
            super::llm_search::view(state, scheme),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
