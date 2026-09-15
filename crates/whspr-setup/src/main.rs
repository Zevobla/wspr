//! whspr's custom Windows installer: a standalone, frameless iced binary in
//! the app's Modernist design language.
//!
//! It's a three-state flow -- Install -> Installing -> Done -- plus a Failure
//! state, drawn on a fixed 900x620 borderless window with a single close
//! mark and a draggable header band (the same borderless-window idiom the
//! app's Hub uses on Windows; see `whspr-app`'s `hub::window_settings` and
//! `hub::caption_windows`). Pressing Install performs the **real** install
//! (see `crate::install`): it copies the embedded app payload
//! (`crate::payload`) into `%LOCALAPPDATA%\whspr`, creates the requested
//! Start-menu / Desktop shortcuts, and writes the `HKCU\...\Run` autostart
//! value. `begin_install` drives those steps off the UI thread via
//! `Task::perform`, advancing the progress bar through the genuine steps and
//! routing any error to the Failure screen. The Windows-specific work is
//! `#[cfg(target_os = "windows")]`; off Windows (the macOS gate) the same UI
//! runs a no-op walk so the crate still builds.
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

mod install;
mod logo;
mod payload;
mod screens;
mod screenshot;
mod state;
mod theme;
mod widgets;

use iced::{window, Element, Size, Task};

use install::Job;
use state::{Message, Screen, State};

/// The window title (shown in the taskbar / alt-tab; the frameless window
/// draws no title bar of its own).
const TITLE: &str = "Install whspr";

/// The installer window is a fixed 900x620 -- never resized, never
/// maximized.
const WINDOW_SIZE: Size = Size::new(900.0, 620.0);

/// A short, deliberate pause before each real install step so the progress
/// bar advances legibly rather than snapping through the (fast) file writes.
/// The steps themselves are real; this only paces them.
const STEP_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

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
        // The lone close mark just exits.
        Message::Close => iced::exit(),
        // "Open whspr" on the Done screen launches the freshly installed app
        // (Windows; a no-op elsewhere) then exits. Best-effort -- a launch
        // failure still closes the installer rather than trapping the user.
        Message::OpenApp => {
            let _ = install::launch_installed_app();
            iced::exit()
        }
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
        Message::StartInstall => begin_install(state),
        Message::InstallStepped {
            jobs,
            index,
            result,
        } => match result {
            // The job at `index` succeeded: advance the bar to the next job's
            // position (or 100% once the plan is exhausted) and dispatch it.
            Ok(()) => {
                let next = index + 1;
                if let Screen::Installing { progress } = &mut state.screen {
                    *progress = jobs.get(next).map_or(100, |job| job.start_progress());
                }
                dispatch(jobs, next)
            }
            // A real write failed: capture where the bar stopped and surface
            // the error on the Failure screen. No panic, nothing half-applied
            // is retried automatically.
            Err(error) => {
                let at = match &state.screen {
                    Screen::Installing { progress } => *progress,
                    _ => 0,
                };
                state.screen = Screen::Failure {
                    detail: Some(error),
                    at,
                };
                Task::none()
            }
        },
        Message::InstallSucceeded => {
            state.screen = Screen::Done;
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

/// Kicks off the **real** install. Reads the three option toggles off the
/// landing screen, builds the ordered job plan (see `crate::install::plan`),
/// switches to the progress screen at the first job's position, and dispatches
/// that job off the UI thread. Each job then chains to the next via
/// `Message::InstallStepped`, ending in `Message::InstallSucceeded` (Done) or a
/// routed error (Failure). Only ever reached from the Install button press --
/// never from screen selection -- so the screenshot harness performs no
/// install.
fn begin_install(state: &mut State) -> Task<Message> {
    let (autostart, start_menu, desktop) = match &state.screen {
        Screen::Install {
            autostart,
            start_menu,
            desktop,
            ..
        } => (*autostart, *start_menu, *desktop),
        // Install can only be pressed from the landing screen.
        _ => return Task::none(),
    };

    let jobs = install::plan(autostart, start_menu, desktop);
    let first = jobs.first().map_or(0, |job| job.start_progress());
    state.screen = Screen::Installing { progress: first };
    dispatch(jobs, 0)
}

/// Runs job `index` of `jobs` off the UI thread, reporting its outcome as
/// `Message::InstallStepped`. When the plan is exhausted, emits
/// `Message::InstallSucceeded` instead.
fn dispatch(jobs: Vec<Job>, index: usize) -> Task<Message> {
    match jobs.get(index).copied() {
        None => Task::done(Message::InstallSucceeded),
        Some(job) => Task::perform(run_job(job), move |result| Message::InstallStepped {
            jobs,
            index,
            result,
        }),
    }
}

/// Performs a single install `job` on a blocking thread (so a slow disk write
/// never stalls iced's executor), after a short `STEP_DELAY` pace so the bar
/// reads legibly. Real work on Windows; a no-op elsewhere (see
/// `crate::install::perform`).
async fn run_job(job: Job) -> Result<(), String> {
    tokio::time::sleep(STEP_DELAY).await;
    tokio::task::spawn_blocking(move || install::perform(job))
        .await
        .map_err(|e| format!("install step failed to run: {e}"))?
}

/// Drives the headless screenshot. The real install advances itself through
/// `Task`s (`crate::dispatch`), so no timer subscription is needed.
fn subscription(state: &State) -> iced::Subscription<Message> {
    screenshot::subscription(state)
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    screens::view(state)
}
