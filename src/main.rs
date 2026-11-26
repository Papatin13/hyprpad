mod apps;
mod config;
mod folders;
mod history;
mod i18n;
mod ui;

use gtk4::prelude::*;
use gtk4::{gio, glib, Application};

const APP_ID: &str = "com.github.hyperpad";

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_activate(|app| {
        // Toggle behavior: if already open, close it cleanly
        let windows = app.windows();
        if !windows.is_empty() {
            for win in windows {
                win.close();
            }
            return;
        }

        ui::build_ui(app);
    });

    app.run()
}
