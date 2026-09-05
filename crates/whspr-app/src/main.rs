mod app;
mod config_ui;
mod devices;
mod history;
mod hotkey_capture;
mod hub;
mod state;
mod stats;

fn main() -> iced::Result {
    app::run()
}
