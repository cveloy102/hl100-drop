//! Login launch through the XDG autostart spec, honoured by GNOME, KDE, Xfce,
//! Cinnamon, MATE, LXQt and most other desktops, for native and Flatpak installs.

use gtk::glib;
use std::{fs, io, path::PathBuf};

/// The session's autostart folder. Inside Flatpak `user_config_dir` points into
/// the sandbox, so use the host one (reachable through `--filesystem=home`).
/// This works on every desktop, unlike the Background portal (GNOME/KDE only).
fn entry() -> PathBuf {
    let config = if crate::in_flatpak() {
        std::env::var_os("HOST_XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| glib::home_dir().join(".config"))
    } else {
        glib::user_config_dir()
    };
    config
        .join("autostart")
        .join(format!("{}.desktop", crate::APP_ID))
}

/// The command a fresh session should run: the AppImage itself when packaged
/// that way (the mounted binary path changes every launch), otherwise our binary.
fn command() -> String {
    if crate::in_flatpak() {
        return format!("flatpak run --command=hl100-drop {} --login", crate::APP_ID);
    }
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
    fs::read_to_string(entry())
        .map(|text| !text.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
}

pub fn set(enable: bool) -> io::Result<()> {
    // A test profile must never change the real session's autostart.
    if std::env::var_os("HL100_SETTINGS_PATH").is_some() {
        return Ok(());
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
