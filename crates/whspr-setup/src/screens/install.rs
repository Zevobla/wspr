//! Screen 1 -- the landing screen. A red header band (wordmark + a two-line
//! caption + the close mark, over a translucent-white rule, with a big
//! "Install whspr." title) sits above a paper body: the pitch, the
//! Install/Options buttons, and -- when Options is expanded -- the three
//! option toggle rows, which push the body down rather than opening a
//! dialog. The footer is pinned to the window bottom.

use iced::widget::{column, container, mouse_area, row, text, Space};
use iced::{Alignment, Background, Element, Length, Padding};

use crate::state::Message;
use crate::theme;
use crate::widgets;

use super::wordmark;

pub fn view(
    expanded: bool,
    autostart: bool,
    start_menu: bool,
    desktop: bool,
) -> Element<'static, Message> {
    column![
        header(expanded),
        body(expanded, autostart, start_menu, desktop),
        widgets::footer()
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The red header band: the wordmark row (brand + caption + close), a 2px
/// translucent-white rule, and the big title. The whole band is the window's
/// drag handle; the close mark captures its own press first. The title
/// shrinks when the options are expanded so the taller body still fits.
fn header(expanded: bool) -> Element<'static, Message> {
    let caption = column![
        text("INSTALL")
            .size(12.0)
            .font(theme::semibold())
            .color(theme::PAPER),
        text("LOCAL VOICE CAPTURE")
            .size(12.0)
            .font(theme::semibold())
            .color(theme::PAPER_MUTED),
    ]
    .spacing(3)
    .align_x(Alignment::End);

    let top = row![
        wordmark(true, 42.0),
        Space::new().width(Length::Fill),
        row![caption, widgets::close_mark(true)]
            .spacing(16)
            .align_y(Alignment::Center),
    ]
    .align_y(Alignment::Center);

    let title_size = if expanded { 44.0 } else { 62.0 };
    let band = column![
        top,
        Space::new().height(Length::Fixed(20.0)),
        widgets::hrule(theme::WHITE_RULE),
        Space::new().height(Length::Fixed(if expanded { 26.0 } else { 32.0 })),
        text("Install whspr.")
            .size(title_size)
            .font(theme::extrabold())
            .color(theme::PAPER),
    ];

    mouse_area(
        container(band)
            .width(Length::Fill)
            .padding(Padding {
                top: 30.0,
                right: 44.0,
                bottom: if expanded { 30.0 } else { 38.0 },
                left: 44.0,
            })
            .style(|_theme| container::Style {
                background: Some(Background::Color(theme::ACCENT)),
                ..container::Style::default()
            }),
    )
    .on_press(Message::Drag)
    .into()
}

/// The paper body: the two-line pitch, the button row, and (when expanded)
/// the option rows. Height-fills so the footer stays pinned to the bottom.
fn body(
    expanded: bool,
    autostart: bool,
    start_menu: bool,
    desktop: bool,
) -> Element<'static, Message> {
    let options_label = if expanded {
        "Options  \u{25B4}"
    } else {
        "Options  \u{25BE}"
    };
    let buttons = row![
        widgets::primary_button("Install", Message::StartInstall),
        widgets::ghost_button(options_label, Message::ToggleOptions),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let mut stack = column![
        text("Hold a key, speak, and clean text appears in whatever you\u{2019}re using.")
            .size(19.0)
            .font(theme::regular())
            .color(theme::INK),
        Space::new().height(Length::Fixed(10.0)),
        text("Installs to your account \u{2014} no administrator needed.")
            .size(15.0)
            .font(theme::regular())
            .color(theme::DIMMED),
        Space::new().height(Length::Fixed(26.0)),
        buttons,
    ]
    .width(Length::Fill);

    if expanded {
        stack = stack
            .push(Space::new().height(Length::Fixed(24.0)))
            .push(options(autostart, start_menu, desktop));
    }

    container(stack)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: if expanded { 26.0 } else { 34.0 },
            right: 44.0,
            bottom: 0.0,
            left: 44.0,
        })
        .into()
}

/// The three option toggle rows, separated by 1px neutral-300 rules.
fn options(autostart: bool, start_menu: bool, desktop: bool) -> Element<'static, Message> {
    column![
        widgets::option_row(
            "Launch whspr when I sign in",
            "Waits in the tray for your hotkey.",
            autostart,
            Message::ToggleAutostart,
        ),
        widgets::hairline(theme::NEUTRAL_300),
        widgets::option_row(
            "Start-menu shortcut",
            "Under All apps \u{2192} whspr.",
            start_menu,
            Message::ToggleStartMenu,
        ),
        widgets::hairline(theme::NEUTRAL_300),
        widgets::option_row(
            "Desktop shortcut",
            "Off by default.",
            desktop,
            Message::ToggleDesktop,
        ),
    ]
    .width(Length::Fill)
    .into()
}
