//! The Models screen: sign in with HuggingFace, then manage ASR and refiner
//! models in two unified sections. Each section has ONE selector listing both
//! local (on-disk) and cloud models together (no local/online toggle), over a
//! management table where curated models can be downloaded or deleted. A final
//! section manages the directories scanned for models. All backed by the
//! `whspr-hf` crate; see `crate::model_menu` for the selector view models.

mod asr;
mod common;
mod dirs;
mod refine;

use iced::widget::{button, column};
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
    } else {
        body = body.push(body_text(
            "Set [huggingface].oauth-client-id in your config to enable sign-in. You can still \
             download the public models below without signing in."
                .to_string(),
            scheme,
        ));
    }

    if let Some(status) = &state.hf_status {
        body = body.push(body_text(status.clone(), scheme));
    }

    section(scheme, "HuggingFace account", body.into())
}

/// A one-line note reporting available RAM, so the per-model "fits your
/// machine" badges below have context.
fn ram_note<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    body_text(
        format!(
            "Your machine reports {} of RAM available. Badges estimate how comfortably each model \
             fits.",
            whspr_hf::human_size(state.hf_specs.available_ram)
        ),
        scheme,
    )
}
