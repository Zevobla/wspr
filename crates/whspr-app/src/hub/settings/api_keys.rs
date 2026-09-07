//! The "API keys" section: plaintext `text_input` fields writing straight
//! into `config.api_keys`, keyed the same way `AsrBackend::id()`/
//! `TextRefiner::id()` name their backend ("openai"/"anthropic"/
//! "deepgram"). Room is left in the map for a future "huggingface" entry,
//! but that key belongs to another stream and is deliberately not added
//! here -- see `Config::api_keys`'s doc comment for why these stay
//! plaintext for now.

use iced::widget::{column, text_input};
use iced::Element;

use crate::hub::common::{field, section};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

/// (backend id, field label), in display order. The id is what gets used
/// as the `config.api_keys` map key.
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

fn api_key_field<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
    id: &'static str,
    label: &'static str,
) -> Element<'a, Message> {
    let value = state
        .config
        .api_keys
        .get(id)
        .map(String::as_str)
        .unwrap_or("");
    let input = text_input("sk-...", value)
        .secure(true)
        .on_input(move |v| Message::ApiKeyChanged(id, v))
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    field(scheme, label, input.into())
}
