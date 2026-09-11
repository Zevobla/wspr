//! The Models screen: sign in with HuggingFace, then manage ASR and refiner
//! models in two unified sections. Each section has ONE selector listing both
//! local (on-disk) and cloud models together (no local/online toggle), over a
//! management table where curated models can be downloaded or deleted. A final
//! section manages the directories scanned for models. All backed by the
//! `whspr-hf` crate; see `crate::model_menu` for the selector view models.

mod asr;
mod common;
mod dirs;
mod llm_search;
mod refine;

use iced::widget::{button, column, text_input};
use iced::Element;

use crate::hub::common::section;
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

use common::{body_text, label_text};

/// Renders the Models screen: the account section, a RAM note, then the ASR,
/// refiner, and model-directory sections.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        account_section(state, scheme),
        ram_note(state, scheme),
        asr::view(state, scheme),
        refine::view(state, scheme),
        dirs::view(state, scheme),
    ]
    .spacing(spacing::XL)
    .into()
}

/// Whether the user is signed in (a live login this session or a saved token).
fn is_signed_in(state: &State) -> bool {
    state.hf_username.is_some() || state.config.huggingface.token.is_some()
}

fn account_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let has_client_id = state
        .config
        .huggingface
        .oauth_client_id
        .as_deref()
        .is_some_and(|id| !id.trim().is_empty());

    let mut body = column![].spacing(spacing::SM);

    if is_signed_in(state) {
        let who = match &state.hf_username {
            Some(name) => format!("Signed in as {name}."),
            None => "Signed in with a saved token.".to_string(),
        };
        body = body.push(body_text(who, scheme));
        body = body.push(
            button(label_text("Sign out"))
                .style(move |_theme, status| styles::button::outlined(scheme, status))
                .on_press_maybe((!state.hf_busy).then_some(Message::HfSignOut)),
        );
    } else if has_client_id {
        body = body.push(body_text(
            "Sign in to download gated models and use your account.".to_string(),
            scheme,
        ));
        let sign_in_label = if state.hf_busy {
            "Waiting for your browser..."
        } else {
            "Sign in with HuggingFace"
        };
        body = body.push(
            button(label_text(sign_in_label))
                .style(move |_theme, status| styles::button::filled(scheme, status))
                .on_press_maybe((!state.hf_busy).then_some(Message::HfSignIn)),
        );
        body = body.push(token_login(state, scheme));
    } else {
        body = body.push(body_text(
            "Sign in to download gated models and use your account. No OAuth app required -- \
             paste a HuggingFace access token below. (You can also enable browser sign-in by \
             setting [huggingface].oauth-client-id in your config.)"
                .to_string(),
            scheme,
        ));
        body = body.push(token_login(state, scheme));
    }

    if let Some(status) = &state.hf_status {
        body = body.push(body_text(status.clone(), scheme));
    }

    section(scheme, "HuggingFace account", body.into())
}

/// The token-paste sign-in control shown in BOTH not-signed-in states: a
/// masked input for a HuggingFace access token plus a submit button, above a
/// hint pointing at the token settings page. This is the primary sign-in path
/// when no OAuth client id is configured, and sits alongside the browser
/// "Sign in with HuggingFace" button when one is. The token is masked
/// (`.secure(true)`) and never leaves `hf_token_input` until `HfTokenSubmit`
/// validates it (see `crate::hf`), which then reuses the OAuth flow's
/// `HfSignedIn` persistence path.
fn token_login<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let input = text_input("hf_...", &state.hf_token_input)
        .secure(true)
        .on_input(Message::HfTokenInput)
        .on_submit(Message::HfTokenSubmit)
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    let can_submit = !state.hf_busy && !state.hf_token_input.trim().is_empty();
    let submit_label = if state.hf_busy {
        "Checking..."
    } else {
        "Sign in with token"
    };
    let submit = button(label_text(submit_label))
        .style(move |_theme, status| styles::button::filled(scheme, status))
        .on_press_maybe(can_submit.then_some(Message::HfTokenSubmit));

    column![
        input,
        submit,
        body_text(
            "Paste an access token from huggingface.co/settings/tokens (a read token is enough)."
                .to_string(),
            scheme,
        ),
    ]
    .spacing(spacing::SM)
    .into()
}

/// A one-line note reporting total RAM and the usable GPU/unified-memory
/// budget models are actually judged against (see `whspr_hf::hardware`), so
/// the per-model "fits your machine" badges below have context.
fn ram_note<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    body_text(
        format!(
            "Your machine: {} RAM, ~{} usable for models. Badges estimate how comfortably each \
             model fits that budget.",
            whspr_hf::human_size(state.hf_specs.total_ram),
            whspr_hf::human_size(state.hf_specs.usable_ram)
        ),
        scheme,
    )
}
