//! Lucide line icons (ISC), rendered through iced's resvg-backed `svg`
//! widget (enabled by the `svg` feature on the existing `iced` dep -- see
//! this crate's `Cargo.toml`). Only the handful of glyphs the UI actually
//! uses are vendored under `assets/icons/`.
//!
//! The small geometric status marks (squares, dots, meter bars) are *not*
//! icons -- they're drawn from `container` + `Border` primitives in
//! `crate::theme::widgets` so they stay crisp and theme-reactive at any
//! DPI. These SVGs are the true line icons that sit inside buttons and rows
//! (`mic`, stop `square`, `clipboard`, ...). Each renders monochrome in a
//! caller-chosen color via `svg::Style { color }`.

use iced::widget::svg::{self, Handle};
use iced::{Color, Element, Length};

pub const MIC: &[u8] = include_bytes!("../../assets/icons/mic.svg");
pub const SQUARE: &[u8] = include_bytes!("../../assets/icons/square.svg");
pub const CLIPBOARD: &[u8] = include_bytes!("../../assets/icons/clipboard.svg");
pub const FILE_TEXT: &[u8] = include_bytes!("../../assets/icons/file-text.svg");
pub const SEARCH: &[u8] = include_bytes!("../../assets/icons/search.svg");
pub const DOWNLOAD: &[u8] = include_bytes!("../../assets/icons/download.svg");
pub const TRASH: &[u8] = include_bytes!("../../assets/icons/trash.svg");

/// A Lucide icon, sized to `size`x`size` and recolored to `color`. Generic
/// over the message type because an icon emits no messages of its own, so
/// it drops straight into any screen's element tree.
pub fn icon<'a, Message: 'a>(data: &'static [u8], size: f32, color: Color) -> Element<'a, Message> {
    svg::Svg::new(Handle::from_memory(data))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_icons_are_real_svgs() {
        for data in [MIC, SQUARE, CLIPBOARD, FILE_TEXT, SEARCH, DOWNLOAD, TRASH] {
            assert!(data.starts_with(b"<svg"), "expected an <svg> asset");
        }
    }
}
