//! The "Privacy" section: microphone-privacy and history-encryption
//! toggles (AG-01, AG-03, M-17).

use iced::widget::{checkbox, column};
use iced::Element;

use crate::hub::common::section;
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let mic_privacy = checkbox(state.config.privacy.mic_privacy)
        .label("Release the microphone when not recording")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::MicPrivacyToggled);

    let history_encryption = checkbox(state.config.privacy.history_encryption)
        .label("Encrypt history at rest")
        .style(move |_theme: &iced::Theme, status| styles::checkbox::field(scheme, status))
        .on_toggle(Message::HistoryEncryptionToggled);

    section(
        scheme,
        "Privacy",
        column![mic_privacy, history_encryption]
            .spacing(spacing::MD)
            .into(),
    )
}
