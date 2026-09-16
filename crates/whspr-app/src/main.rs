// A GUI binary: never attach a console. On Windows the default subsystem is
// "console", which pops a stray terminal alongside the window; force the
// "windows" subsystem so the Hub launches clean. Inert on macOS/Linux.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod active_window;
mod app;
mod config_ui;
mod devices;
mod hf;
mod hf_progress;
mod history;
mod history_encryption;
mod hotkey_capture;
mod hub;
mod link_import;
mod logging;
mod message;
mod model_menu;
mod note_desk;
mod note_export;
mod screen;
mod screenshot;
mod secret_store;
mod sound;
mod speakers;
mod state;
mod stats;
mod system_theme;
mod theme;
mod transcribe_file;
mod transcribe_url;
mod tray;
mod tray_state;
mod worker;
mod worker_events;

fn main() -> iced::Result {
    logging::init();
    app::run()
}
