mod autostart;
mod engine;
mod settings;
mod tray;
mod ui;
mod updates;

use gtk::{gio, glib, prelude::*};
use std::time::{Duration, Instant};

pub const APP_ID: &str = "ru.hl100.drop";

/// Running inside the Flatpak sandbox (autostart and tray need other routes).
pub fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

fn main() -> glib::ExitCode {
    // After a self-update the new image waits for the old process to release the
    // single-instance D-Bus name; otherwise it would just hand its arguments over.
    if let Some(pid) = std::env::var("HL100_WAIT_PID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
    {
        std::env::remove_var("HL100_WAIT_PID");
        let deadline = Instant::now() + Duration::from_secs(15);
        while std::path::Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    glib::set_application_name("DROP");
    let mut flags = gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    // Separate settings = separate device; lets two copies run side by side for tests.
    if std::env::var_os("HL100_SETTINGS_PATH").is_some() {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_startup(ui::startup);
    app.connect_command_line(|app, line| {
        let mut launch = ui::Launch::default();
        let mut args = line.arguments().into_iter().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--login") => launch.login = true,
                Some("--quit") => launch.quit = true,
                Some("--send-to") => {
                    launch.send_to = args.next().map(|a| a.to_string_lossy().into())
                }
                Some("--") => {}
                // Relative paths are relative to the launching process, not to us.
                _ => {
                    if let Some(path) = line.create_file_for_arg(&arg).path() {
                        launch.files.push(path.to_string_lossy().into());
                    }
                }
            }
        }
        ui::open(app, launch);
        glib::ExitCode::SUCCESS
    });
    app.run()
}
