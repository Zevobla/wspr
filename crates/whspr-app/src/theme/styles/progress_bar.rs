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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_progress_bar_uses_tertiary_colors() {
        let scheme = &color::LIGHT;
        let style = thinking(scheme);
        // Verify that the bar and background are from the tertiary palette
        assert_eq!(style.bar, Background::Color(scheme.tertiary));
    }

    #[test]
    fn thinking_progress_bar_has_container_background() {
        let scheme = &color::LIGHT;
        let style = thinking(scheme);
        assert_eq!(style.background, Background::Color(scheme.tertiary_container));
    }
}
