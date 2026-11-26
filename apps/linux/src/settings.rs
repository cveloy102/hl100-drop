use gtk::glib;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub configured: bool,
    pub check_updates: bool,
    pub notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let downloads = glib::user_special_dir(glib::UserDirectory::Downloads)
            .unwrap_or_else(|| glib::home_dir().join("Downloads"));
        Self {
            id: glib::uuid_string_random().replace('-', ""),
            name: glib::host_name().to_string(),
            folder: downloads.join("HL100 Drop").to_string_lossy().into(),
            configured: false,
            check_updates: true,
            notifications: true,
        }
    }
}

impl Settings {
    pub fn location() -> PathBuf {
        if let Some(path) = std::env::var_os("HL100_SETTINGS_PATH") {
            return path.into();
        }
        glib::user_config_dir()
            .join("hl100-drop")
            .join("settings.json")
    }

    /// A broken or missing file falls back to defaults: first launch shows setup.
    pub fn load() -> Self {
        let loaded = fs::read(Self::location())
            .ok()
            .and_then(|data| serde_json::from_slice::<Settings>(&data).ok());
        match loaded {
            Some(mut s) => {
                if s.id.trim().is_empty() {
                    s.id = Settings::default().id;
                }
                s
            }
            None => Settings::default(),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let location = Self::location();
        if let Some(parent) = location.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = location.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temp, location)
    }
}

/// Create the folder and prove we can write there before automatic reception starts.
pub fn validate_folder(folder: &str) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    let probe = PathBuf::from(folder).join(format!(".hl100-{}", glib::uuid_string_random()));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    fs::remove_file(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_partial_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("hl100-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, br#"{"name":"Kali","configured":true}"#).unwrap();
        let parsed: Settings = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed.name, "Kali");
        assert!(parsed.configured && parsed.check_updates && parsed.notifications);
        validate_folder(dir.join("in/box").to_str().unwrap()).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
