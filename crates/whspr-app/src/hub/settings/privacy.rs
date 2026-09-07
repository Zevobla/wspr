//! The "Privacy" section: microphone-privacy and history-encryption
//! toggles (AG-01, AG-03, M-17).

use iced::widget::column;
use iced::Element;

use crate::hub::common::{section, toggle_row};
use crate::state::{Message, State};
use crate::theme::{color, spacing};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    section(
        scheme,
        "Privacy",
        column![
            toggle_row(
                scheme,
                "Release the microphone when not recording",
                state.config.privacy.mic_privacy,
                Message::MicPrivacyToggled,
            ),
            toggle_row(
                scheme,
                "Encrypt history at rest",
                state.config.privacy.history_encryption,
                Message::HistoryEncryptionToggled,
            ),
        ]
        .spacing(spacing::MD)
        .into(),
    )
}
