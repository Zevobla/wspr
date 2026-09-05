mod app;
mod config_ui;
mod devices;
mod flow_bar;
mod history;
mod hotkey_capture;
mod hub;
mod state;
mod stats;
mod worker;

fn main() -> iced::Result {
    app::run()
}
