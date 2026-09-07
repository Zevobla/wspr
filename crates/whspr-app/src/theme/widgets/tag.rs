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
    /// error-container fill, on-error-container text, 1px error border --
    /// the loudest tag, for a hard "this won't work" verdict (e.g. a model
    /// that won't fit the machine). Distinct from [`TagKind::Accent`] even
    /// though Modernist keeps a single accent hue, since it borrows the
    /// same solid-fill-plus-border treatment as the error banner rather
    /// than a plain tint.
    Error,
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
        TagKind::Error => body
            .style(move |_theme| styles::container::tag_error(scheme))
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_builds_for_every_kind() {
        for kind in [
            TagKind::Accent,
            TagKind::Neutral,
            TagKind::Outline,
            TagKind::Error,
        ] {
            let _: Element<'_, ()> = tag(kind, "Active", &color::LIGHT);
        }
    }
}
