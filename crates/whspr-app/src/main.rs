mod active_window;
mod app;
mod config_ui;
mod devices;
mod flow_bar;
mod history;
mod hotkey_capture;
mod hub;
mod logging;
mod sound;
mod speakers;
mod state;
mod stats;
mod theme;
mod tray;
mod worker;

fn main() -> iced::Result {
    logging::init();
    app::run()
}
