//! The Modernist `.tag` -- a small, square, tinted label. Three kinds map
//! onto the ramp fills in `styles::container` (accent / neutral / outline).

use iced::widget::{container, text};
use iced::Element;

use crate::theme::{color, styles, type_scale};

/// Which tint a tag carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// accent-100 fill, accent-800 text -- "Active", "Needed".
    Accent,
    /// neutral-100 fill, neutral-800 text -- quiet metadata.
    Neutral,
    /// transparent, 1px accent border + accent text -- "On this Mac".
    Outline,
}

/// A tag pill (square): 11px tracked label, 3x10 padding, zero radius.
pub fn tag<'a, M: 'a>(
    kind: TagKind,
    label: impl Into<String>,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let body = container(
        text(label.into())
            .size(type_scale::KICKER.size)
            .font(type_scale::KICKER.font()),
    )
    .padding([3.0, 10.0]);
    match kind {
        TagKind::Accent => body
            .style(move |_theme| styles::container::tag_accent(scheme))
            .into(),
        TagKind::Neutral => body
            .style(move |_theme| styles::container::tag_neutral(scheme))
            .into(),
        TagKind::Outline => body
            .style(move |_theme| styles::container::tag_outline(scheme))
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_builds_for_every_kind() {
        for kind in [TagKind::Accent, TagKind::Neutral, TagKind::Outline] {
            let _: Element<'_, ()> = tag(kind, "Active", &color::LIGHT);
        }
    }
}
