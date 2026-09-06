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

#[cfg(test)]
mod tests {
    #[test]
    fn all_modules_compile() {
        // Verify that all declared modules compile and are accessible.
        // This is a smoke test ensuring the module structure is correct.
        // Note: We don't call main() in tests since it requires iced's GUI.
    }
}
