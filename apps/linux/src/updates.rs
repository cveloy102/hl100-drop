//! Update checks against GitHub Releases, like the other platforms.
//!
//! An AppImage replaces itself after a SHA-256 check. A .deb/.rpm or source
//! install is owned by the package manager, so we only open the release page.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::Duration,
};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "cveloy102/hl100-drop";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub appimage: Option<String>,
    pub digest: String,
    pub checksums: Option<String>,
}

pub fn appimage_name(version: &str) -> String {
    format!("HL100Drop-{version}-x86_64.AppImage")
}

/// The AppImage file we were started from, if any.
pub fn running_appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20 * 60)))
        .user_agent(format!("HL100Drop/{CURRENT} (Linux)"))
        .build()
        .into()
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse().ok();
    Some((next()?, next()?, next().unwrap_or(0)))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

pub fn check() -> Result<Option<Release>, String> {
    let mut response = match agent()
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_release(&root)
}

pub fn parse_release(root: &Value) -> Result<Option<Release>, String> {
    let version = root["tag_name"]
        .as_str()
        .ok_or("Некорректный ответ GitHub")?
        .trim_start_matches('v')
        .to_string();
    if !is_newer(&version, CURRENT) {
        return Ok(None);
    }
    let assets = root["assets"].as_array().cloned().unwrap_or_default();
    let find = |name: &str| assets.iter().find(|a| a["name"] == name).cloned();
    let image = find(&appimage_name(&version));
    Ok(Some(Release {
        notes: root["body"].as_str().unwrap_or("").into(),
        page: root["html_url"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://github.com/{REPOSITORY}/releases/tag/v{version}")),
        appimage: image
            .as_ref()
            .and_then(|a| a["browser_download_url"].as_str().map(String::from)),
        digest: image
            .as_ref()
            .and_then(|a| a["digest"].as_str().map(String::from))
            .unwrap_or_default(),
        checksums: find("checksums.txt")
            .and_then(|a| a["browser_download_url"].as_str().map(String::from)),
        version,
    }))
}

fn validate_url(url: &str) -> Result<(), String> {
    let prefix = format!("https://github.com/{REPOSITORY}/releases/download/");
    if url.starts_with(&prefix) && !url[prefix.len()..].contains("..") {
        Ok(())
    } else {
        Err("Некорректный адрес обновления".into())
    }
}

fn expected_digest(release: &Release, file_name: &str) -> Result<String, String> {
    if let Some(hex) = release.digest.strip_prefix("sha256:") {
        if hex.len() == 64 {
            return Ok(hex.to_ascii_lowercase());
        }
    }
    if let Some(url) = &release.checksums {
        validate_url(url)?;
        let text = agent()
            .get(url)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            if let (Some(hash), Some(name)) = (parts.next(), parts.next()) {
                if name.trim_start_matches('*') == file_name && hash.len() == 64 {
                    return Ok(hash.to_ascii_lowercase());
                }
            }
        }
    }
    Err("В релизе отсутствует контрольная сумма AppImage".into())
}

/// Download the new AppImage beside the running one, verify it and swap it in.
/// Returns the path to relaunch. `progress` receives 0..=100.
pub fn install_appimage(release: &Release, progress: impl Fn(f64)) -> Result<PathBuf, String> {
    let target = running_appimage().ok_or("DROP запущен не из AppImage")?;
    let url = release.appimage.as_deref().ok_or("В релизе нет AppImage")?;
    validate_url(url)?;
    let digest = expected_digest(release, &appimage_name(&release.version))?;
    let part = target.with_extension("AppImage.part");
    let result = (|| -> Result<(), String> {
        let mut response = agent().get(url).call().map_err(|e| e.to_string())?;
        let total = response.body().content_length().unwrap_or(0);
        let mut input = response.body_mut().with_config().limit(1 << 31).reader();
        let mut output = fs::File::create(&part).map_err(|e| e.to_string())?;
        let (mut hasher, mut buffer, mut done) = (Sha256::new(), vec![0u8; 1 << 20], 0u64);
        loop {
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            hasher.update(&buffer[..n]);
            done += n as u64;
            if total > 0 {
                progress(done as f64 * 100.0 / total as f64);
            }
        }
        output.sync_all().map_err(|e| e.to_string())?;
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if hash != digest {
            return Err("Контрольная сумма обновления не совпала".into());
        }
        fs::set_permissions(&part, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        // Same directory → atomic rename; the running image stays mapped until exit.
        fs::rename(&part, &target).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result.map(|_| target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("v2.0", "1.1.3"));
        assert!(!is_newer("1.1.3", "1.1.3"));
        assert!(!is_newer("junk", "1.0.0"));
    }

    #[test]
    fn picks_appimage_asset() {
        let name = appimage_name("99.0.0");
        let url = format!("https://github.com/{REPOSITORY}/releases/download/v99.0.0/{name}");
        let root = json!({"tag_name":"v99.0.0","body":"notes","assets":[
            {"name":name,"browser_download_url":url,"digest":format!("sha256:{}", "a".repeat(64))}
        ]});
        let release = parse_release(&root).unwrap().unwrap();
        assert_eq!(release.appimage.as_deref(), Some(url.as_str()));
        assert_eq!(expected_digest(&release, &name).unwrap(), "a".repeat(64));
        assert!(validate_url(&url).is_ok());
        assert!(validate_url("https://evil.example/x").is_err());
        assert!(parse_release(&json!({"tag_name":"v0.0.1"}))
            .unwrap()
            .is_none());
    }
}
