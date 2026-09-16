//! The "API keys" section: one row per cloud backend, keyed the same way
//! `AsrBackend::id()`/`TextRefiner::id()` name their backend
//! ("openai"/"anthropic"/"deepgram"). A key is typed into a masked draft
//! field and saved explicitly -- into the OS keystore when that survives a
//! reboot, else into `config.toml` (see `crate::secret_store`) -- and is never
//! rendered back: the row only says where the saved key lives.

use iced::widget::{button, column, row, text, text_input};
use iced::{Alignment, Element};

use crate::hub::common::{field, section};
use crate::secret_store::{keystore_label, SecretLocation, SecretSlot};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

/// (backend id, field label), in display order. Every id is one of
/// `crate::secret_store::MANAGED_SECRETS`.
const API_KEY_FIELDS: [(&str, &str); 3] = [
    ("openai", "OpenAI API key"),
    ("anthropic", "Anthropic API key"),
    ("deepgram", "Deepgram API key"),
];

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let fields: Vec<Element<'a, Message>> = API_KEY_FIELDS
        .iter()
        .map(|&(id, label)| api_key_field(state, scheme, id, label))
        .collect();

    section(
        scheme,
        "API keys",
        column(fields).spacing(spacing::MD).into(),
    )
}

/// The caption under a key row saying where its saved key lives.
fn location_caption(location: SecretLocation) -> String {
    match location {
        SecretLocation::Keystore => format!("Saved \u{2014} stored in {}", keystore_label()),
        SecretLocation::ConfigFile => "Saved in config.toml (plaintext)".to_string(),
        SecretLocation::Missing => "Not set".to_string(),
    }
}

fn api_key_field<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
    id: &'static str,
    label: &'static str,
) -> Element<'a, Message> {
    let location = state
        .secret_locations
        .get(&SecretSlot::ApiKey(id))
        .copied()
        .unwrap_or(SecretLocation::Missing);
    let draft = state
        .api_key_drafts
        .get(id)
        .map(String::as_str)
        .unwrap_or("");
    let placeholder = match location {
        SecretLocation::Missing => "sk-...",
        SecretLocation::Keystore | SecretLocation::ConfigFile => "Paste a new key to replace it",
    };

    let input = text_input(placeholder, draft)
        .secure(true)
        .on_input(move |v| Message::ApiKeyDraftChanged(id, v))
        .on_submit(Message::ApiKeySaved(id))
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));
    let save = button(label_text("Save"))
        .style(move |_theme, status| styles::button::filled(scheme, status))
        .on_press_maybe((!draft.trim().is_empty()).then_some(Message::ApiKeySaved(id)));
    let mut controls = row![input, save]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
    if location != SecretLocation::Missing {
        controls = controls.push(
            button(label_text("Remove"))
                .style(move |_theme, status| styles::button::text(scheme, status))
                .on_press(Message::ApiKeyRemoved(id)),
        );
    }

    let caption = text(location_caption(location))
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant);

    field(
        scheme,
        label,
        column![controls, caption].spacing(spacing::XS).into(),
    )
}

/// A button label in the section's label type.
fn label_text<'a>(label: &'static str) -> iced::widget::Text<'a> {
    text(label)
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_api_key_row_is_a_managed_secret() {
        for (id, _) in API_KEY_FIELDS {
            assert!(
                crate::secret_store::MANAGED_SECRETS.contains(&SecretSlot::ApiKey(id)),
                "{id} is not in MANAGED_SECRETS"
            );
        }
    }

    #[test]
    fn captions_name_the_store_without_showing_the_key() {
        assert!(location_caption(SecretLocation::Keystore).contains(keystore_label()));
        assert!(location_caption(SecretLocation::ConfigFile).contains("config.toml"));
        assert_eq!(location_caption(SecretLocation::Missing), "Not set");
    }
}
