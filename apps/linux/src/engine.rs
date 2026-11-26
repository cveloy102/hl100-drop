//! Bridge to the shared Rust core. Every call runs off the GTK thread.

use gtk::gio;
use serde_json::Value;

/// Run one core command (see docs/protocol.md) on a worker thread.
pub async fn call(command: Value) -> Result<Value, String> {
    let input = command.to_string();
    let result = gio::spawn_blocking(move || hl100_core::dispatch(&input))
        .await
        .map_err(|_| "Ошибка ядра".to_string())?;
    if result["ok"] == Value::Bool(false) {
        return Err(result["error"]
            .as_str()
            .unwrap_or("Ошибка ядра")
            .to_string());
    }
    Ok(result)
}

/// Stop the engine synchronously; used on the way out of the process.
pub fn stop_blocking() {
    let _ = hl100_core::dispatch(r#"{"op":"stop"}"#);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub platform: String,
}

impl Peer {
    pub fn parse(value: &Value) -> Option<Self> {
        let port = value["port"].as_u64()?;
        if !(1..=65535).contains(&port) {
            return None;
        }
        Some(Self {
            id: value["id"].as_str()?.into(),
            name: value["name"].as_str()?.into(),
            address: value["address"].as_str()?.into(),
            port: port as u16,
            platform: value["platform"].as_str().unwrap_or("unknown").into(),
        })
    }
}

pub fn bytes(value: f64) -> String {
    const UNITS: [&str; 5] = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
    let (mut number, mut index) = (value.max(0.0), 0);
    while number >= 1024.0 && index < UNITS.len() - 1 {
        number /= 1024.0;
        index += 1;
    }
    let text = format!("{number:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text} {}", UNITS[index])
}

/// Total hours, not the 0-23 component: long transfers must not wrap.
pub fn eta(seconds: f64) -> String {
    let total = seconds.clamp(0.0, 864_000.0) as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        total / 60 % 60,
        total % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_sizes_and_time() {
        assert_eq!(bytes(0.0), "0 Б");
        assert_eq!(bytes(1536.0), "1.5 КБ");
        assert_eq!(bytes(1024.0 * 1024.0), "1 МБ");
        assert_eq!(eta(3.0), "00:00:03");
        assert_eq!(eta(90_061.0), "25:01:01");
    }

    #[test]
    fn rejects_bad_peers() {
        let ok =
            json!({"id":"a","name":"Mac","address":"10.0.0.2","port":41000,"platform":"macos"});
        assert_eq!(Peer::parse(&ok).unwrap().platform, "macos");
        assert!(Peer::parse(&json!({"id":"a","name":"x","address":"1","port":0})).is_none());
        assert!(Peer::parse(&json!({"name":"x","address":"1","port":1})).is_none());
    }
}
