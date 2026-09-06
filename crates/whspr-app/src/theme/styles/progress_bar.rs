//! Progress bar style for the Flow Bar's "Thinking" sweep.

use iced::widget::progress_bar::Style;
use iced::{Background, Border};

use crate::theme::color;

pub fn thinking(scheme: &color::Scheme) -> Style {
    Style {
        background: Background::Color(scheme.tertiary_container),
        bar: Background::Color(scheme.tertiary),
        border: Border::default(),
    }
}
