//! Login launch through the XDG autostart spec, honoured by GNOME, KDE, Xfce,
//! Cinnamon, MATE, LXQt and most other desktops.

use gtk::{gio, glib, prelude::*};
use std::{fs, io, path::PathBuf};

fn sandboxed() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Inside Flatpak the host autostart folder is out of reach; the Background
/// portal writes the entry for us. The portal cannot be queried, so we keep a marker.
fn portal_marker() -> PathBuf {
    glib::user_config_dir().join("hl100-drop").join("autostart")
}

fn portal_request(enable: bool) -> io::Result<()> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(io::Error::other)?;
    let options = glib::VariantDict::new(None);
    options.insert("reason", "Приём файлов после входа в систему");
    options.insert("autostart", enable);
    options.insert(
        "commandline",
        vec!["hl100-drop".to_string(), "--login".to_string()],
    );
    options.insert("dbus-activatable", false);
    bus.call_sync(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&("", options.end()).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
    )
    .map_err(io::Error::other)?;
    if enable {
        fs::create_dir_all(portal_marker().parent().unwrap())?;
        fs::write(portal_marker(), b"1")
    } else {
        match fs::remove_file(portal_marker()) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

fn entry() -> PathBuf {
    glib::user_config_dir()
        .join("autostart")
        .join(format!("{}.desktop", crate::APP_ID))
}

/// The command a fresh session should run: the AppImage itself when packaged
/// that way (the mounted binary path changes every launch), otherwise our binary.
fn command() -> String {
    let exe = std::env::var("APPIMAGE")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().into())
        })
        .unwrap_or_else(|| "hl100-drop".into());
    // Desktop Entry spec: quote and escape reserved characters.
    let escaped = exe
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$");
    format!("\"{escaped}\" --login")
}

pub fn enabled() -> bool {
    if sandboxed() {
        return portal_marker().exists();
    }
    fs::read_to_string(entry())
        .map(|text| !text.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
}

pub fn set(enable: bool) -> io::Result<()> {
    // A test profile must never change the real session's autostart.
    if std::env::var_os("HL100_SETTINGS_PATH").is_some() {
        return Ok(());
    }
    if sandboxed() {
        return portal_request(enable);
    }
    let path = entry();
    if !enable {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        path,
        format!(
            "[Desktop Entry]\nType=Application\nName=DROP\nComment=Приём файлов по локальной сети\n\
             Exec={}\nIcon={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            command(),
            crate::APP_ID
        ),
    )
}
