//! Live HuggingFace GGUF search UI for the refiner (LLM) section: a query
//! field over a results list, where selecting a repo lists its `.gguf` files
//! (size + "fits your machine" badge) each with a Download button that feeds
//! the existing LLM download -> rescan path (see `crate::hf`). A downloaded
//! community model then shows up in the refiner selector like a curated one.
//!
//! Kept in its own module so `refine.rs`, `hf.rs`, and `state.rs` all stay
//! under the AA-06 600-line cap.

use iced::widget::{button, column, row, text, text_input};
use iced::{Alignment, Element, Length};

use crate::hf::LlmSearchState;
use crate::state::{Message, State};
use crate::theme::{color, icons, spacing, styles, type_scale};

use super::common::{body_text, catalog_table, download_button, label_text, CatalogRow};

/// The search block appended under the curated refiner catalog: an intro line,
/// the query field + Search button, a status/loading/error/"no results" line,
/// then the repo hits (each expandable to its `.gguf` files).
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let search = &state.llm_search;

    let input = text_input("Search HuggingFace for a GGUF model...", &search.query)
        .on_input(Message::LlmSearchInput)
        .on_submit(Message::LlmSearchSubmit)
        .width(Length::Fill)
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    let can_search = !search.busy && !search.query.trim().is_empty();
    let search_button = button(
        row![
            icons::icon(icons::SEARCH, 14.0, scheme.on_secondary_container),
            label_text("Search"),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    )
    .style(move |_theme, status| styles::button::tonal(scheme, status))
    .on_press_maybe(can_search.then_some(Message::LlmSearchSubmit));

    let mut block = column![
        body_text(
            "Search HuggingFace for any community GGUF refiner and download it \
             straight into your models directory."
                .to_string(),
            scheme,
        ),
        row![input, search_button]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
    ]
    .spacing(spacing::MD);

    if let Some(status) = status_line(search) {
        block = block.push(body_text(status, scheme));
    }

    for hit in &search.results {
        block = block.push(repo_row(state, scheme, hit));
    }

    block.into()
}

/// The single status line under the search box: the last error, an in-flight
/// "Searching..." note, or a "no results" line once a search has returned
/// empty. `None` (nothing shown) otherwise.
fn status_line(search: &LlmSearchState) -> Option<String> {
    if let Some(error) = &search.error {
        return Some(error.clone());
    }
    if search.busy && search.results.is_empty() {
        return Some("Searching HuggingFace...".to_string());
    }
    if search.searched && search.results.is_empty() {
        return Some("No GGUF models found for that search.".to_string());
    }
    None
}

/// One repo hit: its id + download count, a Show/Hide-files toggle, and -- when
/// expanded -- the repo's `.gguf` file table below.
fn repo_row<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
    hit: &'a whspr_hf::GgufRepoHit,
) -> Element<'a, Message> {
    let selected = state.llm_search.selected_repo.as_deref() == Some(hit.id.as_str());
    let toggle_label = if selected { "Hide files" } else { "Show files" };
    let repo_id = hit.id.clone();

    let header = row![
        column![
            text(hit.id.as_str())
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font())
                .color(scheme.on_surface),
            text(format!("{} downloads", human_count(hit.downloads)))
                .size(type_scale::LABEL_MEDIUM.size)
                .font(type_scale::LABEL_MEDIUM.font())
                .color(scheme.on_surface_variant),
        ]
        .spacing(spacing::XS)
        .width(Length::Fill),
        button(label_text(toggle_label))
            .style(move |_theme, status| styles::button::outlined(scheme, status))
            .on_press_maybe(
                (!state.llm_search.busy).then_some(Message::LlmSearchSelectRepo(repo_id))
            ),
    ]
    .spacing(spacing::MD)
    .align_y(Alignment::Center);

    if selected {
        column![header, files_view(state, scheme, &hit.id)]
            .spacing(spacing::SM)
            .into()
    } else {
        header.into()
    }
}

/// The expanded repo's `.gguf` files as a catalog table (name / size / fit /
/// Download), or a loading/"no files" line while none are available yet. The
/// Download button routes into the existing LLM download -> rescan path.
fn files_view<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
    repo: &str,
) -> Element<'a, Message> {
    let search = &state.llm_search;
    if search.busy && search.files.is_empty() {
        return body_text("Loading files...".to_string(), scheme);
    }
    if search.files.is_empty() {
        return body_text("No .gguf files in this repo.".to_string(), scheme);
    }

    let rows: Vec<CatalogRow<'a>> = search
        .files
        .iter()
        .map(|file| CatalogRow {
            name: file.file_name().to_string(),
            size_bytes: file.size_bytes,
            fit: state.hf_specs.fit(file.size_bytes, whspr_hf::ModelKind::Llm),
            action: download_button(
                scheme,
                state.hf_busy,
                Message::LlmSearchDownload(repo.to_string(), file.path.clone()),
            ),
        })
        .collect();
    catalog_table(scheme, rows)
}

/// Formats a download count compactly: `98765` -> `"98.8k"`, `2_100_000` ->
/// `"2.1M"`, small counts verbatim.
fn human_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::human_count;

    #[test]
    fn human_count_scales_thousands_and_millions() {
        assert_eq!(human_count(42), "42");
        assert_eq!(human_count(98_765), "98.8k");
        assert_eq!(human_count(2_100_000), "2.1M");
    }
}
