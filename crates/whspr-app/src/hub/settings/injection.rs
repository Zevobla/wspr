//! The "Injection" section: text-injection timing -- currently just the
//! pre-paste delay (AM-20), read from `config.injection.pre_paste_delay_ms`.

use iced::widget::{column, text_input};
use iced::Element;

use crate::hub::common::{field, section};
use crate::state::{Message, State};
use crate::theme::{color, spacing, styles};

pub(super) fn view<'a>(state: &'a State, scheme: &'static color::Scheme) -> Element<'a, Message> {
    let pre_paste_delay = text_input("0", &state.pre_paste_delay_draft)
        .on_input(Message::PrePasteDelayMsChanged)
        .style(move |_theme, status| styles::text_input::outlined(scheme, status));

    section(
        scheme,
        "Injection",
        column![field(
            scheme,
            "Pre-paste delay (ms)",
            pre_paste_delay.into()
        )]
        .spacing(spacing::MD)
        .into(),
    )
}
