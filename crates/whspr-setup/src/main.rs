//! whspr's custom Windows installer: a standalone, frameless iced binary in
//! the app's Modernist design language.
//!
//! It's a three-state flow -- Install -> Installing -> Done -- plus a Failure
//! state, drawn on a fixed 900x620 borderless window with a single close
//! mark and a draggable header band (the same borderless-window idiom the
//! app's Hub uses on Windows; see `whspr-app`'s `hub::window_settings` and
//! `hub::caption_windows`). The install actions themselves (file copy,
//! shortcuts, registry) are **simulated** this pass -- a timed progress that
//! walks the steps -- behind a clearly marked seam (`begin_install`) so the
//! real Windows logic can slot in later without touching the UI.
//!
//! Built on `iced::daemon` (like `whspr-app`'s `crate::app`) rather than
//! `iced::application`: `daemon`'s `view` is handed the `window::Id`, which
//! the headless screenshot self-validation (`crate::screenshot`) needs to
//! capture the window.

// A frameless GUI installer: never attach a console. On Windows the default
// subsystem is "console", which pops a stray terminal alongside the window;
// force the "windows" subsystem so the installer launches clean. Inert on
// macOS/Linux.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod logo;
mod screens;
mod screenshot;
mod state;
mod theme;
mod widgets;

use iced::{window, Element, Size, Task};

use state::{Message, Screen, State};

/// The window title (shown in the taskbar / alt-tab; the frameless window
/// draws no title bar of its own).
const TITLE: &str = "Install whspr";

/// The installer window is a fixed 900x620 -- never resized, never
/// maximized.
const WINDOW_SIZE: Size = Size::new(900.0, 620.0);

/// The simulated install advances one percent every `TICK` and cycles the
/// step label at the 25/50/75 thresholds, so a full 0->100 run takes ~2.5s.
const TICK: std::time::Duration = std::time::Duration::from_millis(25);

fn main() -> iced::Result {
    iced::daemon(boot, update, view)
        .title(TITLE)
        // Register the three static Archivo faces and make Regular the
        // default so every surface renders in the Modernist type family and
        // each weight resolves to its own crisp static face (see
        // `crate::theme`).
        .font(theme::ARCHIVO_REGULAR)
        .font(theme::ARCHIVO_SEMIBOLD)
        .font(theme::ARCHIVO_EXTRABOLD)
        .default_font(theme::DEFAULT)
        .theme(theme_of)
        .subscription(subscription)
        .run()
}

/// The installer only ever renders in the light (paper) Modernist scheme.
/// A free `fn` (not a closure) so it satisfies the daemon's higher-ranked
/// `for<'a> Fn(&'a State, _) -> Theme` bound cleanly.
fn theme_of(_state: &State, _window: window::Id) -> iced::Theme {
    iced::Theme::Light
}

fn boot() -> (State, Task<Message>) {
    let state = State::from_env();
    let (_id, open) = window::open(window_settings());
    (state, open.map(Message::WindowOpened))
}

/// The installer window: fixed 900x620, borderless (`decorations: false`),
/// and not resizable -- no OS title bar, min/maximize, or resize border. The
/// app draws its own close mark and wires window drag on the header band
/// (see `crate::screens`). `min_size == max_size == size` belt-and-braces
/// pins the size even on WMs that ignore `resizable: false`.
fn window_settings() -> window::Settings {
    window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(WINDOW_SIZE),
        max_size: Some(WINDOW_SIZE),
        resizable: false,
        decorations: false,
        ..window::Settings::default()
    }
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::WindowOpened(id) => {
            state.window = Some(id);
            Task::none()
        }
        // The header band is the drag handle -- there is no OS title bar.
        Message::Drag => match state.window {
            Some(id) => window::drag(id),
            None => Task::none(),
        },
        // The lone close mark, and "Open whspr" on the Done screen, both
        // exit cleanly for now. `// TODO: launch installed app` -- OpenApp
        // will spawn the installed binary once the real install lands.
        Message::Close | Message::OpenApp => iced::exit(),
        Message::ToggleOptions => {
            if let Screen::Install { expanded, .. } = &mut state.screen {
                *expanded = !*expanded;
            }
            Task::none()
        }
        Message::ToggleAutostart => {
            if let Screen::Install { autostart, .. } = &mut state.screen {
                *autostart = !*autostart;
            }
            Task::none()
        }
        Message::ToggleStartMenu => {
            if let Screen::Install { start_menu, .. } = &mut state.screen {
                *start_menu = !*start_menu;
            }
            Task::none()
        }
        Message::ToggleDesktop => {
            if let Screen::Install { desktop, .. } = &mut state.screen {
                *desktop = !*desktop;
            }
            Task::none()
        }
        Message::StartInstall => {
            begin_install(state);
            Task::none()
        }
        Message::Tick => {
            if let Screen::Installing { progress } = &mut state.screen {
                *progress = progress.saturating_add(1);
                if *progress >= 100 {
                    state.screen = Screen::Done;
                }
            }
            Task::none()
        }
        Message::Retry => {
            state.screen = Screen::install_default();
            Task::none()
        }
        Message::TakeScreenshot => match state.window {
            Some(id) if !state.screenshot_taken => {
                state.screenshot_taken = true;
                window::screenshot(id).map(Message::ScreenshotTaken)
            }
            _ => Task::none(),
        },
        Message::ScreenshotTaken(shot) => {
            if let Some(path) = state.screenshot_path.clone() {
                if let Err(e) = screenshot::save(&path, &shot) {
                    eprintln!("whspr-setup screenshot failed: {e}");
                }
            }
            iced::exit()
        }
    }
}

/// Kicks off the (simulated) install: switch to the progress screen at 0%.
///
/// **Real-install seam.** Today this only flips the screen; the
/// `Message::Tick` subscription then walks the bar to 100%. The real Windows
/// implementation slots in here -- spawn the actual copy/shortcut/registry
/// work (behind `#[cfg(target_os = "windows")]`) reporting progress back
/// through the same `Message::Tick`/progress channel, and route a genuine
/// write error to `Screen::Failure` instead of `Screen::Done`. The three
/// `Screen::Install` option bools (autostart / start_menu / desktop) are the
/// inputs that logic will read.
fn begin_install(state: &mut State) {
    // TODO: real install -- replace the timed simulation below with the
    // actual Windows file copy / shortcut creation / registry writes,
    // reporting progress through `Message::Tick`.
    state.screen = Screen::Installing { progress: 0 };
}

/// Drives the simulated install and the headless screenshot. The install
/// timer is suppressed while a screenshot is pending so the captured frame
/// stays at the fixed progress the harness seeded (see
/// `crate::state::screen_from_env`).
fn subscription(state: &State) -> iced::Subscription<Message> {
    let ticking = matches!(state.screen, Screen::Installing { .. })
        && state.screenshot_path.is_none();
    let install = if ticking {
        iced::time::every(TICK).map(|_| Message::Tick)
    } else {
        iced::Subscription::none()
    };
    iced::Subscription::batch([install, screenshot::subscription(state)])
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    screens::view(state)
}
