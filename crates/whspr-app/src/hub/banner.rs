//! The under-header status banner: the red worker-error notice, or the
//! calm first-run "pick a model" prompt. Split out of `hub/mod.rs` to keep
//! that file under the AA-06 line cap.

use iced::widget::{container, text, Space};
use iced::{Element, Length};

use crate::state::{Message, State};
use crate::theme::{color, spacing, styles, type_scale};

/// The under-header status banner. A genuine worker error takes precedence
/// and shows the red error notice; a fresh install with no model yet shows a
/// calm onboarding prompt instead; otherwise nothing. Keeping the two cases
/// visually distinct means "no model configured" reads as first-run guidance,
/// not as a failure (the bug this fixes).
pub(super) fn status_banner<'a>(
    state: &'a State,
    scheme: &'static color::Scheme,
) -> Element<'a, Message> {
    if let Some(error) = &state.last_error {
        banner(
            format!("Last worker error: {error}"),
            styles::container::error_banner(scheme),
        )
    } else if state.needs_model {
        banner(
            "Pick a speech model in Models to start dictating.".to_string(),
            styles::container::onboarding_banner(scheme),
        )
    } else {
        Space::new().into()
    }
}

/// A full-width notice band under the header, carrying `message` in the given
/// container `style`. Shared by both the error and onboarding cases of
/// [`status_banner`] so only the copy and style differ.
fn banner<'a>(message: String, style: iced::widget::container::Style) -> Element<'a, Message> {
    container(
        container(
            text(message)
                .size(type_scale::BODY_MEDIUM.size)
                .font(type_scale::BODY_MEDIUM.font()),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| style),
    )
    .padding([spacing::SM, spacing::XXL])
    .into()
}
