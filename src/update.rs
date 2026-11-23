//! GitHub release check plus one-click install of the platform package.
//!
//! The check runs off the UI thread; the install path downloads only from the
//! release assets themselves and refuses asset names that are not plain files.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const API: &str = "https://api.github.com/repos/Zwart04/omnix/releases/latest";
const UA: &str = "omnix-updater";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

struct Update {
    version: String,
    asset: Asset,
}

/// Latest release when it is newer than this build, otherwise `None`.
fn check() -> Result<Option<Update>> {
    let release: Release =
        serde_json::from_str(&fetch(API)?).context("parse the GitHub release")?;
    let version = release.tag_name.trim_start_matches('v').to_string();
    if !newer(&version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let Some(asset) = pick_asset(&release.assets) else {
        bail!("release {version} has no package for this platform");
    };
    Ok(Some(Update {
        version,
        asset: Asset {
            name: asset.name.clone(),
            browser_download_url: asset.browser_download_url.clone(),
        },
    }))
}

/// Payload for the `updateCheck` RPC.
pub fn check_payload() -> Result<Value> {
    let current = env!("CARGO_PKG_VERSION");
    Ok(match check()? {
        Some(update) => json!({
            "available": true,
            "current": current,
            "version": update.version,
        }),
        None => json!({"available": false, "current": current}),
    })
}

/// Download and hand the release package to the platform installer.
///
/// `restart` is used on Linux, where a writeable binary is replaced in place
/// and a fresh process is spawned; Windows quits so the installer can finish.
pub fn install(restart: bool) -> Result<Value> {
    let Some(update) = check()? else {
        return Ok(json!({
            "ok": true,
            "quit": false,
            "message": format!("Omnix {} is up to date.", env!("CARGO_PKG_VERSION")),
        }));
    };
    let bytes = download(&update.asset.browser_download_url)?;
    install_asset(&update.asset.name, &bytes, &update.version, restart)
}

fn fetch(url: &str) -> Result<String> {
    let response = ureq::get(url)
        .set("User-Agent", UA)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .call()
        .with_context(|| format!("GET {url}"))?;
    response.into_string().context("read the GitHub response")
}

fn download(url: &str) -> Result<Vec<u8>> {
    if !url.starts_with("https://github.com/")
        && !url.starts_with("https://objects.githubusercontent.com/")
    {
        bail!("refusing to download from {url}");
    }
    let response = ureq::get(url)
        .set("User-Agent", UA)
        .timeout(Duration::from_secs(120))
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(300 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .context("read the release download")?;
    if bytes.is_empty() {
        bail!("empty download from {url}");
    }
    Ok(bytes)
}

fn pick_asset(assets: &[Asset]) -> Option<&Asset> {
    if cfg!(target_os = "windows") {
        return assets
            .iter()
            .find(|asset| asset.name.ends_with(".exe") && asset.name.contains("setup"));
    }
    if cfg!(target_os = "macos") {
        return assets.iter().find(|asset| asset.name.ends_with(".dmg"));
    }
    // Linux updates prefer the raw binary so a user-local install can replace
    // itself; fall back to the native package opened by the system installer.
    let arch = Path::new("/etc/arch-release").exists();
    let debian = Path::new("/etc/debian_version").exists();
    assets
        .iter()
        .find(|asset| asset.name == "omnix-linux-x64")
        .or_else(|| {
            assets.iter().find(|asset| {
                if arch {
                    asset.name.ends_with(".pkg.tar.zst")
                } else if debian {
                    asset.name.ends_with(".deb")
                } else {
                    false
                }
            })
        })
}

fn temp_file(name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let name = safe_name(name)?;
    let dir = std::env::temp_dir().join("omnix-update");
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(name);
    fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn safe_name(name: &str) -> Result<&str> {
    let path = Path::new(name);
    if name.is_empty() || path.file_name().and_then(|part| part.to_str()) != Some(name) {
        bail!("refusing unsafe asset name '{name}'");
    }
    Ok(name)
}

fn newer(candidate: &str, current: &str) -> bool {
    parts(candidate) > parts(current)
}

fn parts(version: &str) -> (u64, u64, u64) {
    let mut parts = version.trim_start_matches('v').split('.').map(|part| {
        part.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(target_os = "windows")]
fn install_asset(name: &str, bytes: &[u8], _version: &str, _restart: bool) -> Result<Value> {
    let path = temp_file(name, bytes)?;
    Command::new(&path)
        .args(["/SILENT", "/CLOSEAPPLICATIONS", "/NORESTART"])
        .spawn()
        .with_context(|| format!("run {}", path.display()))?;
    Ok(json!({
        "ok": true,
        "quit": true,
        "message": "Installer started. Omnix will close and update.",
    }))
}

#[cfg(target_os = "macos")]
fn install_asset(name: &str, bytes: &[u8], version: &str, _restart: bool) -> Result<Value> {
    let path = temp_file(name, bytes)?;
    Command::new("open")
        .arg(&path)
        .spawn()
        .context("open the disk image")?;
    Ok(json!({
        "ok": true,
        "quit": true,
        "message": format!("Omnix {version} disk image opened. Drag Omnix into Applications."),
    }))
}

#[cfg(target_os = "linux")]
fn install_asset(name: &str, bytes: &[u8], version: &str, restart: bool) -> Result<Value> {
    if name == "omnix-linux-x64" && replace_exe(bytes).is_ok() {
        if restart {
            if let Ok(exe) = std::env::current_exe() {
                let _ = Command::new(exe).spawn();
            }
        }
        return Ok(json!({
            "ok": true,
            "quit": restart,
            "message": format!("Omnix {version} installed."),
        }));
    }
    let path = temp_file(name, bytes)?;
    let opened = Command::new("xdg-open")
        .arg(&path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    Ok(if opened {
        json!({
            "ok": true,
            "quit": false,
            "message": format!("Omnix {version} package opened in your installer."),
        })
    } else {
        json!({
            "ok": false,
            "quit": false,
            "message": format!("Package saved to {}. Install it with your package manager.", path.display()),
        })
    })
}

#[cfg(target_os = "linux")]
fn replace_exe(bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let exe = std::env::current_exe().context("locate the running binary")?;
    let staged = exe.with_extension("new");
    fs::write(&staged, bytes).with_context(|| format!("write {}", staged.display()))?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    if let Err(err) = fs::rename(&staged, &exe) {
        let _ = fs::remove_file(&staged);
        return Err(err).with_context(|| format!("replace {}", exe.display()));
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn install_asset(_name: &str, _bytes: &[u8], _version: &str, _restart: bool) -> Result<Value> {
    bail!("self-update is not supported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_compares_numerically() {
        assert!(newer("0.6.0", "0.5.9"));
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(!newer("0.6.0", "0.6.0"));
        assert!(!newer("0.5.9", "0.6.0"));
    }

    #[test]
    fn asset_names_stay_in_the_temp_dir() {
        assert!(safe_name("omnix-setup-x64.exe").is_ok());
        assert!(safe_name("../evil").is_err());
        assert!(safe_name("").is_err());
    }
}
