//! The Modernist `.table`: a column of rows with an uppercase kicker
//! header over a 2px rule, and 1px hairlines between body rows. iced has no
//! native table, so this lays cells out with per-column `Length` widths.

use iced::widget::{column, container, row, text, Column};
use iced::{Element, Length};

use crate::theme::{color, spacing, styles, type_scale};

use super::hairline;

/// A data table. `columns` pairs each header label with its column width;
/// every row in `rows` must have one pre-built cell element per column.
pub fn table<'a, M: 'a>(
    columns: Vec<(&'a str, Length)>,
    rows: Vec<Vec<Element<'a, M>>>,
    scheme: &'static color::Scheme,
) -> Element<'a, M> {
    let header = row(columns.iter().map(|(label, width)| {
        container(
            text(label.to_uppercase())
                .size(type_scale::KICKER.size)
                .font(type_scale::KICKER.font())
                .color(scheme.on_surface_variant),
        )
        .width(*width)
        .padding([spacing::SM, spacing::SM])
        .into()
    }))
    .align_y(iced::Alignment::Center);

    let widths: Vec<Length> = columns.iter().map(|(_, w)| *w).collect();

    let mut body: Column<'a, M> = column![
        header,
        // The header's strong 2px rule.
        strong_rule(scheme),
    ];

    for cells in rows {
        let laid_out = row(cells.into_iter().enumerate().map(|(i, cell)| {
            container(cell)
                .width(widths.get(i).copied().unwrap_or(Length::Fill))
                .padding([spacing::SM, spacing::SM])
                .into()
        }))
        .align_y(iced::Alignment::Center);
        body = body.push(laid_out);
        body = body.push(hairline(scheme));
    }

    body.width(Length::Fill).into()
}

fn strong_rule<'a, M: 'a>(scheme: &'static color::Scheme) -> Element<'a, M> {
    container(iced::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(spacing::layout::RULE))
        .style(move |_theme| styles::container::divider(scheme))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_builds_with_rows() {
        let cols = vec![("When", Length::Fixed(80.0)), ("What", Length::Fill)];
        let rows: Vec<Vec<Element<'_, ()>>> = vec![vec![
            text("09:12").into(),
            text("hello there").into(),
        ]];
        let _: Element<'_, ()> = table(cols, rows, &color::LIGHT);
    }
}
