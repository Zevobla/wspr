//! The under-header status banners: the red worker-error notice, a calm
//! dismissible notice (`State::notice`), and the first-run "pick a model"
//! prompt. Split out of `hub/mod.rs` to keep that file under the AA-06 line
//! cap.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

/// One kind of banner the Hub can show under its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BannerKind {
    /// The last worker error, in the red error style.
    Error,
    /// A calm informational notice the user can dismiss.
    Notice,
    /// The first-run "pick a speech model" prompt.
    Onboarding,
}

/// The banners `state` calls for, top to bottom. A genuine worker error
/// comes first and suppresses the onboarding prompt (so "no model
/// configured" never competes with a real failure); a notice shows
/// alongside either, since it reports something that just happened.
fn visible_banners(state: &State) -> Vec<BannerKind> {
    let mut kinds = Vec::new();
    if state.last_error.is_some() {
        kinds.push(BannerKind::Error);
    }
    if state.notice.is_some() {
        kinds.push(BannerKind::Notice);
    }
    if state.last_error.is_none() && state.needs_model {
        kinds.push(BannerKind::Onboarding);
    }
    kinds
}

/// The under-header status banners (see [`visible_banners`] for which show
/// and in what order); an empty column when there is nothing to report.
/// Errors and onboarding keep visually distinct styles so "no model
/// configured" reads as first-run guidance, not as a failure.
pub(super) fn status_banner<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    let banners = visible_banners(state).into_iter().map(|kind| match kind {
        BannerKind::Error => banner(
            format!(
                "Last worker error: {}",
                state.last_error.as_deref().unwrap_or_default()
            ),
            styles::container::error_banner(scheme),
            None,
        ),
        BannerKind::Notice => banner(
            state.notice.clone().unwrap_or_default(),
            styles::container::onboarding_banner(scheme),
            Some(dismiss_button(scheme)),
        ),
        BannerKind::Onboarding => banner(
            "Pick a speech model in Models to start dictating.".to_string(),
            styles::container::onboarding_banner(scheme),
            None,
        ),
    });
    column(banners).into()
}

/// The notice banner's trailing "Dismiss" action.
fn dismiss_button<'a>(scheme: &'static color::Scheme) -> Element<'a, Message> {
    button(
        text("Dismiss")
            .size(type_scale::LABEL_LARGE.size)
            .font(type_scale::LABEL_LARGE.font()),
    )
    .style(move |_theme, status| styles::button::text(scheme, status))
    .on_press(Message::DismissNotice)
    .into()
}

/// A full-width notice band under the header, carrying `message` in the given
/// container `style`, with an optional trailing `action`. Shared by every
/// [`BannerKind`] so only the copy, style and action differ.
fn banner<'a>(
    message: String,
    style: iced::widget::container::Style,
    action: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut content = row![text(message)
        .size(type_scale::BODY_MEDIUM.size)
        .font(type_scale::BODY_MEDIUM.font())
        .width(Length::Fill)]
    .align_y(Alignment::Center)
    .spacing(spacing::MD);
    if let Some(action) = action {
        content = content.push(action);
    }
    container(
        container(content)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(move |_theme| style),
    )
    .padding([spacing::SM, spacing::XXL])
    .into()
}

