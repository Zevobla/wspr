//! The whspr logo mark, in its two Modernist variants. Two colorways share
//! one 1024 viewBox: the `on_red` variant sits on the accent header band
//! (paper "w", ink block, paper bar); the paper variant is inverted for the
//! Installing/Done/Failure headers (paper ground, ink "w", accent block, ink
//! bar). The mark is a lowercase Archivo ExtraBold "w" with a small block
//! biting its top-right and a bar crossing its lower third -- the same
//! construction as the app's `assets/icon.svg`.
//!
//! The geometric rects are drawn with iced's `svg` widget from an inline SVG
//! string, but the "w" is overlaid as an iced `text`, not baked into the
//! SVG: iced's svg widget resolves fonts from a *separate* fontdb that only
//! holds system fonts, so an SVG `<text font-family="Archivo">` renders in a
//! fallback face on any machine without Archivo installed (e.g. the macOS
//! render box). Overlaying the glyph as real text guarantees it is the same
//! embedded Archivo ExtraBold face the wordmark beside it uses -- the mark
//! and the "whspr" wordmark stay one type family. The rects and the glyph
//! together reproduce the specified SVG exactly.

use iced::widget::svg::{Handle, Svg};
use iced::widget::{container, stack, text};
use iced::{Alignment, Element, Length, Padding};

use crate::theme;

/// Builds the SVG markup for the mark's geometric rects (ground, top-right
/// block, lower bar) -- everything but the "w", which is overlaid as text.
/// `on_red` picks the header-band colorway; `false` the inverted paper one.
fn rects_markup(on_red: bool) -> String {
    let (ground, block, bar) = if on_red {
        ("#ec3013", "#201e1d", "#f3f2f2")
    } else {
        ("#f3f2f2", "#ec3013", "#201e1d")
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><rect x="0" y="0" width="1024" height="1024" fill="{ground}"/><rect x="784" y="552" width="152" height="152" fill="{block}"/><rect x="0" y="704" width="1024" height="40" fill="{bar}"/></svg>"##
    )
}

/// The logo mark as a `size`x`size` square. `on_red` selects the header-band
/// colorway (paper glyph on accent); `false` the inverted paper colorway.
pub fn logo<'a, Message: 'a>(on_red: bool, size: f32) -> Element<'a, Message> {
    let w_color = if on_red { theme::PAPER } else { theme::INK };
    let base = Svg::new(Handle::from_memory(rects_markup(on_red).into_bytes()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));
    // The SVG "w" sits at x=96/1024 with its baseline at y=712/1024, set at
    // font-size 800/1024. Reproduce that placement with the overlaid glyph:
    // left-inset ~9.4%, a hair above vertical centre, size ~0.78 of the box.
    let glyph = container(
        text("w")
            .size(size * 0.78)
            .font(theme::extrabold())
            .color(w_color)
            .line_height(text::LineHeight::Relative(1.0)),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .align_x(Alignment::Start)
    .align_y(Alignment::Center)
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: size * 0.04,
        left: size * 0.085,
    });
    stack![base, glyph]
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_and_paper_variants_swap_ground_and_block() {
        let red = rects_markup(true);
        let paper = rects_markup(false);
        assert!(red.contains(r##"fill="#ec3013"/><rect x="784""##));
        assert!(paper.contains(r##"fill="#f3f2f2"/><rect x="784""##));
        // The small block bites in the accent on paper, in ink on red.
        assert!(red.contains(r##"width="152" height="152" fill="#201e1d"/>"##));
        assert!(paper.contains(r##"width="152" height="152" fill="#ec3013"/>"##));
    }
}
