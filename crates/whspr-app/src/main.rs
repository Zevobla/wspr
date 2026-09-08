mod app;
mod config_ui;
mod devices;
mod hf;
mod history;
mod hotkey_capture;
mod hub;
mod logging;
mod model_menu;
mod screenshot;
mod sound;
mod speakers;
mod state;
mod stats;
mod system_theme;
mod theme;
mod transcribe_file;
mod tray;
mod worker;

fn main() -> iced::Result {
    logging::init();
    app::run()
}
