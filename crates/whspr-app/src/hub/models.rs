//! The Models screen: sign in with HuggingFace, browse a curated set of
//! whisper.cpp ASR models with a size + "fits your machine" badge, download
//! one, and point dictation at it -- all backed by the `whspr-hf` crate.

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length};

use crate::state::{Message, State};
use crate::theme::{color, shape, spacing, styles, type_scale};

use super::common::section;

/// Renders the Models screen: the account section over the model list.
pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    column![
        account_section(state, scheme),
        models_section(state, scheme),
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

fn models_section<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let rows: Vec<Element<'a, Message>> = whspr_hf::MODELS
        .iter()
        .map(|model| model_row(model, state, scheme))
        .collect();

    section(
        scheme,
        "Models",
        column![
            body_text(
                format!(
                    "Your machine reports {} of RAM available. Badges estimate how comfortably \
                     each model fits.",
                    whspr_hf::human_size(state.hf_specs.available_ram)
                ),
                scheme,
            ),
            column(rows).spacing(spacing::MD),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}

fn model_row<'a>(
    model: &'static whspr_hf::WhisperModel,
    state: &State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let installed = state
        .hf_installed
        .iter()
        .find(|m| m.filename == model.filename);
    let in_use = installed.is_some_and(|m| {
        state
            .config
            .whisper
            .model_path
            .as_deref()
            .is_some_and(|p| p == m.path)
    });

    let name = text(model.label)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface)
        .width(Length::Fixed(200.0));

    let size = text(whspr_hf::human_size(model.size_bytes))
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .width(Length::Fixed(80.0));

    let badge = fit_badge(state.hf_specs.fit(model.size_bytes), scheme);

    let action: Element<'a, Message> = if in_use {
        body_text("In use".to_string(), scheme)
    } else if let Some(m) = installed {
        let path = m.path.clone();
        button(label_text("Use this model"))
            .style(move |_theme, status| styles::button::filled(scheme, status))
            .on_press_maybe((!state.hf_busy).then_some(Message::HfUseModel(path)))
            .into()
    } else {
        button(label_text("Download"))
            .style(move |_theme, status| styles::button::tonal(scheme, status))
            .on_press_maybe((!state.hf_busy).then_some(Message::HfDownloadModel(model.id)))
            .into()
    };

    row![name, size, badge, Space::new().width(Length::Fill), action,]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
}

/// A small colored "fits your machine" pill for a [`whspr_hf::Fit`] verdict:
/// green (`success_container`), yellow (`tertiary_container`), red
/// (`error_container`), reusing the scheme's tonal container roles.
fn fit_badge(fit: whspr_hf::Fit, scheme: &'static color::Scheme) -> Element<'static, Message> {
    let (bg, fg) = match fit {
        whspr_hf::Fit::Green => (scheme.success_container, scheme.on_success_container),
        whspr_hf::Fit::Yellow => (scheme.tertiary_container, scheme.on_tertiary_container),
        whspr_hf::Fit::Red => (scheme.error_container, scheme.on_error_container),
    };

    container(
        text(fit.label())
            .size(type_scale::LABEL_MEDIUM.size)
            .font(type_scale::LABEL_MEDIUM.font())
            .color(fg),
    )
    .padding(spacing::XS)
    .style(move |_theme| iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: Border::default().rounded(shape::SM),
        ..Default::default()
    })
    .into()
}

/// A `BODY_MEDIUM`, de-emphasized paragraph -- the wording used across this
/// screen's status/help lines.
fn body_text(content: String, scheme: &'static color::Scheme) -> Element<'static, Message> {
    text(content)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .color(scheme.on_surface_variant)
        .into()
}

/// A `LABEL_LARGE` button label, matching the other screens' buttons.
fn label_text(content: &'static str) -> Element<'static, Message> {
    text(content)
        .size(type_scale::LABEL_LARGE.size)
        .font(type_scale::LABEL_LARGE.font())
        .into()
}
