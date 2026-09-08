//! Reusable Modernist widgets built from iced primitives -- the shared
//! component surface every Hub screen draws from. Split
//! into small submodules (marks, tags, controls, table, nav) so no file
//! approaches the 600-line cap, and re-exported flat here so call sites
//! write `widgets::tag(..)`, `widgets::toggle(..)`, `widgets::hr(..)`.

pub mod controls;
pub mod marks;
pub mod nav;
pub mod table;
pub mod tag;

pub use controls::{segmented, toggle};
pub use marks::{meter, status_square, Mark};
pub use nav::screen_header;
pub use table::table;
pub use tag::{tag, TagKind};

use iced::widget::{container, text, Space};
use iced::{Border, Element, Length};

use crate::theme::{color, spacing, styles, type_scale};

/// A strong 2px horizontal rule -- the system's primary divider between
/// major sections.
pub fn hr<'a, M: 'a>(scheme: &'static color::Scheme) -> Element<'a, M> {
    horizontal_rule(scheme, spacing::layout::RULE)
}

/// A 1px hairline rule -- between table rows and adjacent columns.
pub fn hairline<'a, M: 'a>(scheme: &'static color::Scheme) -> Element<'a, M> {
    horizontal_rule(scheme, spacing::layout::HAIRLINE)
}

fn horizontal_rule<'a, M: 'a>(scheme: &'static color::Scheme, height: f32) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .style(move |_theme| styles::container::divider(scheme))
        .into()
}

/// A vertical rule `width`px wide spanning the parent's height -- the rail's
/// right edge and the Settings sub-nav's right edge (iced `Border` is
/// uniform, so single-side rules are drawn as sibling containers).
pub fn vrule<'a, M: 'a>(width: f32, scheme: &'static color::Scheme) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(move |_theme| styles::container::divider(scheme))
        .into()
}

/// A bordered keycap for the hotkey display ("⌥", "Space").
pub fn kbd<'a, M: 'a>(label: impl Into<String>, scheme: &'static color::Scheme) -> Element<'a, M> {
    container(
        text(label.into())
            .size(type_scale::BODY_MEDIUM.size)
            .font(type_scale::LABEL_LARGE.font())
            .color(scheme.on_surface),
    )
    .padding([2.0, 8.0])
    .style(move |_theme| container::Style {
        background: Some(iced::Background::Color(scheme.surface_container)),
        border: Border {
            color: scheme.outline,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}
