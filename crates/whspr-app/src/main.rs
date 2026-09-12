mod app;
mod config_ui;
mod devices;
mod hf;
mod hf_progress;
mod history;
mod hotkey_capture;
mod hub;
mod link_import;
mod logging;
mod model_menu;
mod note_desk;
mod screen;
mod screenshot;
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

fn main() -> iced::Result {
    logging::init();
    app::run()
}
