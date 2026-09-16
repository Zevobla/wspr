//! Subscriptions wired up in `crate::app::run` (`iced::daemon(...).subscription(...)`),
//! split out of `app.rs` to keep it under this project's 600-line-per-file
//! guideline (AA-06).

use crate::state::{Message, State};

/// Only listens for keyboard events while the Hub is actively capturing a
/// hotkey preview, so normal typing elsewhere in the Hub doesn't get
/// swallowed or misread as a capture attempt the rest of the time.
fn hotkey_capture_subscription(state: &State) -> iced::Subscription<Message> {
    if state.hotkey_capturing {
        iced::keyboard::listen().map(Message::HotkeyCaptureKeyEvent)
    } else {
        iced::Subscription::none()
    }
}

/// The pipeline worker runs for the whole lifetime of the app (it owns the
/// hotkey listener), independent of Hub UI state.
fn worker_subscription(_state: &State) -> iced::Subscription<Message> {
    iced::Subscription::run(crate::worker::pipeline_worker).map(Message::Worker)
}

/// Polls the tray icon for pending menu clicks (see `crate::tray`'s module
/// doc comment for why this is polled rather than pushed). Only runs once
/// `state.tray` actually exists -- `None` on Linux, or if creation failed
/// -- so there's nothing to poll for the app's whole life there. 5Hz is
/// plenty responsive for a "Show Hub"/"Quit" click.
fn tray_poll_subscription(state: &State) -> iced::Subscription<Message> {
    if state.tray.is_some() {
        iced::time::every(std::time::Duration::from_millis(200)).map(|_| Message::TrayPoll)
    } else {
        iced::Subscription::none()
    }
}

/// Keeps the tray's lingering "Done" display on-screen for
/// `TRAY_DONE_LINGER` after a completed dictation (see `Message::Worker`'s
/// `Completed` arm), then reverts it via `Message::TrayDoneTick`. Only
/// ticks while a linger is actually pending -- same idiom as
/// `tray_poll_subscription`/`mic_level_subscription` -- so an idle tray
/// costs nothing the rest of the time.
fn tray_done_subscription(state: &State) -> iced::Subscription<Message> {
    if state.tray_done_until.is_some() {
        iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::TrayDoneTick)
    } else {
        iced::Subscription::none()
    }
}

pub(super) fn subscription(state: &State) -> iced::Subscription<Message> {
    iced::Subscription::batch([
        hotkey_capture_subscription(state),
        worker_subscription(state),
        tray_poll_subscription(state),
        tray_done_subscription(state),
        mic_level_subscription(state),
        link_import_key_subscription(state),
        hub_close_subscription(state),
        crate::screenshot::subscription(state),
        crate::system_theme::subscription(state),
    ])
}

/// Intercepts the Hub window's OS close request (`exit_on_close_request` is
/// off -- see `crate::hub::window_settings`) so `Message::HubCloseRequested`
/// can hide it to the tray instead of quitting (macOS/Windows) or exit
/// cleanly (Linux). Without this the daemon would strand a closed window.
fn hub_close_subscription(_state: &State) -> iced::Subscription<Message> {
    iced::window::close_requests().map(|_id| Message::HubCloseRequested)
}

/// While the "Add from a link" modal is open, listens for keyboard events so
/// Esc can dismiss it (see `crate::link_import`). Scoped to the open dialog so
/// it never swallows keys the rest of the time.
fn link_import_key_subscription(state: &State) -> iced::Subscription<Message> {
    if state.link_import.is_some() {
        iced::keyboard::listen().map(Message::LinkImportKey)
    } else {
        iced::Subscription::none()
    }
}

/// While the in-app Record button is capturing, ticks ~12x/sec so the view
/// refreshes `mic_level` for the live meter; idle otherwise.
fn mic_level_subscription(state: &State) -> iced::Subscription<Message> {
    if state.is_recording {
        iced::time::every(std::time::Duration::from_millis(80)).map(|_| Message::MicLevelTick)
    } else {
        iced::Subscription::none()
    }
}
