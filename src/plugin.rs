//! Community plugins: a manifest, an on-demand child process, and a JSON-RPC
//! request over stdio. Nothing here starts at boot. An unused plugin costs no
//! memory, no CPU, and no background thread.
//!
//! Plugins are native programs installed by the owner, not a kernel sandbox.
//! Omnix enforces the manifest at the gate (API version, enabled flag, revoked
//! permissions), passes only a small environment allowlist, and never hands a
//! plugin agent tokens, cookies, or the owner's control socket token.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const API_VERSION: u32 = 1;
pub const DEFAULT_TIMEOUT_MS: u64 = 15_000;
pub const MAX_TIMEOUT_MS: u64 = 120_000;
pub const DEFAULT_RAM_MB: u64 = 256;
const MAX_MANIFEST: u64 = 256 * 1024;
const MAX_RESPONSE: u64 = 4 * 1024 * 1024;
const MAX_FILE: u64 = 8 * 1024 * 1024;
const PUBLIC_CATALOG: &str =
    "https://raw.githubusercontent.com/Zwart04/omnix-plugins/main/index.json";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn plugins_dir() -> PathBuf {
    crate::control::home().join(".omnix").join("plugins")
}

pub fn state_path() -> PathBuf {
    plugins_dir().join("state.json")
}

pub fn data_dir(name: &str) -> PathBuf {
    plugins_dir().join("data").join(name)
}

pub fn plugin_dir(name: &str) -> PathBuf {
    plugins_dir().join(name)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_version(version: &str) -> bool {
    let core = version
        .split_once('+')
        .map(|(left, _)| left)
        .unwrap_or(version)
        .split_once('-')
        .map(|(left, _)| left)
        .unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn valid_arg(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn safe_rel(value: &str) -> bool {
    if value.is_empty() || value.len() > 512 || value.starts_with('/') || value.contains('\\') {
        return false;
    }
    let path = Path::new(value);
    path.components().all(|part| match part {
        Component::Normal(part) => !part.is_empty(),
        _ => false,
    })
}

/// One `[[contributions]]` table. Unknown kinds are rejected by serde, so a
/// newer plugin cannot silently pretend an old Omnix understands it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Contribution {
    Command {
        #[serde(default)]
        name: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        description: String,
    },
    Agent {
        #[serde(default)]
        name: String,
        #[serde(default)]
        bin: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        resume: Vec<String>,
    },
    BrowserTool {
        #[serde(default)]
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        input_schema: Option<Value>,
    },
    Hook {
        #[serde(default)]
        events: Vec<String>,
    },
    Panel {
        #[serde(default)]
        name: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        entry: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Permissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub agents: bool,
    #[serde(default)]
    pub history: bool,
}

impl Permissions {
    pub fn requested(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        if !self.read.is_empty() {
            keys.insert("read".into());
        }
        if !self.write.is_empty() {
            keys.insert("write".into());
        }
        if !self.network.is_empty() {
            keys.insert("network".into());
        }
        if self.agents {
            keys.insert("agents".into());
        }
        if self.history {
            keys.insert("history".into());
        }
        keys
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.read.is_empty() {
            parts.push(format!("read {}", self.read.join(", ")));
        }
        if !self.write.is_empty() {
            parts.push(format!("write {}", self.write.join(", ")));
        }
        if !self.network.is_empty() {
            parts.push(format!("network {}", self.network.join(", ")));
        }
        if self.agents {
            parts.push("run agents".into());
        }
        if self.history {
            parts.push("read history".into());
        }
        if parts.is_empty() {
            "none".into()
        } else {
            parts.join("; ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Runtime {
    #[serde(default)]
    pub entry: Vec<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub ram_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub description: String,
    pub api: u32,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub contributions: Vec<Contribution>,
    #[serde(default)]
    pub permissions: Permissions,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Manifest = toml::from_str(text).context("parse omnix-plugin.toml")?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("omnix-plugin.toml");
        let meta = fs::metadata(&path).with_context(|| format!("read {}", path.display()))?;
        if meta.len() > MAX_MANIFEST {
            bail!("manifest is too large");
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn validate(&self) -> Result<()> {
        if !valid_name(&self.name) {
            bail!("plugin name must be 1-48 chars of a-z, 0-9, '-'");
        }
        if !valid_version(&self.version) {
            bail!("version must be semver like 1.2.3");
        }
        if self.api != API_VERSION {
            bail!(
                "plugin API {} is not supported (this Omnix speaks API {API_VERSION})",
                self.api
            );
        }
        for (field, value) in [
            ("author", &self.author),
            ("license", &self.license),
            ("description", &self.description),
        ] {
            if value.trim().is_empty() || value.len() > 2000 {
                bail!("{field} is required and must stay under 2000 chars");
            }
        }
        if self.runtime.entry.is_empty() {
            bail!("runtime.entry is required");
        }
        for arg in &self.runtime.entry {
            if !valid_arg(arg) {
                bail!("runtime.entry contains an empty or control argument");
            }
        }
        if !self.contributions.is_empty() {
            let mut commands = BTreeSet::new();
            for contribution in &self.contributions {
                match contribution {
                    Contribution::Command { name, .. } => {
                        let name = if name.trim().is_empty() {
                            &self.name
                        } else {
                            name
                        };
                        if !valid_name(name) {
                            bail!("command contribution name must be a-z, 0-9, '-'");
                        }
                        if !commands.insert(name.clone()) {
                            bail!("duplicate command contribution '{name}'");
                        }
                    }
                    Contribution::Agent { name, bin, args, env, resume } => {
                        if !valid_name(name) {
                            bail!("agent contribution name must be a-z, 0-9, '-'");
                        }
                        if bin.trim().is_empty() || !valid_arg(bin) {
                            bail!("agent contribution '{name}' needs a bin");
                        }
                        if args.iter().any(|arg| !valid_arg(arg))
                            || resume.iter().any(|arg| !valid_arg(arg))
                        {
                            bail!("agent contribution '{name}' has a bad argument");
                        }
                        for (key, value) in env {
                            if !valid_env_key(key) || value.chars().any(char::is_control) {
                                bail!("agent contribution '{name}' has a bad env pair");
                            }
                        }
                    }
                    Contribution::BrowserTool { name, .. } => {
                        if !valid_name(name) {
                            bail!("browser-tool name must be a-z, 0-9, '-'");
                        }
                    }
                    Contribution::Hook { events } => {
                        if events.is_empty() {
                            bail!("hook contribution needs at least one event");
                        }
                        for event in events {
                            if event != "*"
                                && (event.is_empty()
                                    || event.len() > 80
                                    || event.chars().any(char::is_control))
                            {
                                bail!("invalid hook event '{event}'");
                            }
                        }
                    }
                    Contribution::Panel { name, entry, .. } => {
                        if !valid_name(name) {
                            bail!("panel name must be a-z, 0-9, '-'");
                        }
                        if !safe_rel(entry) {
                            bail!("panel entry must be a relative path inside the plugin");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn commands(&self) -> Vec<(&str, &str)> {
        self.contributions
            .iter()
            .filter_map(|c| match c {
                Contribution::Command { name, title, .. } => {
                    let name = if name.trim().is_empty() {
                        self.name.as_str()
                    } else {
                        name.as_str()
                    };
                    Some((name, if title.trim().is_empty() { name } else { title }))
                }
                _ => None,
            })
            .collect()
    }

    pub fn panels(&self) -> Vec<(&str, &str, &str)> {
        self.contributions
            .iter()
            .filter_map(|c| match c {
                Contribution::Panel { name, title, entry } => {
                    Some((name.as_str(), title.as_str(), entry.as_str()))
                }
                _ => None,
            })
            .collect()
    }

    pub fn agents(&self) -> Vec<&Contribution> {
        self.contributions
            .iter()
            .filter(|c| matches!(c, Contribution::Agent { .. }))
            .collect()
    }

    pub fn hooks(&self) -> Vec<&Contribution> {
        self.contributions
            .iter()
            .filter(|c| matches!(c, Contribution::Hook { .. }))
            .collect()
    }

    pub fn browser_tools(&self) -> Vec<&Contribution> {
        self.contributions
            .iter()
            .filter(|c| matches!(c, Contribution::BrowserTool { .. }))
            .collect()
    }
}

fn valid_env_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with(|c: char| c.is_ascii_digit())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub installed_at: u64,
    #[serde(default)]
    pub source: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub grants: BTreeMap<String, bool>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub plugins: BTreeMap<String, Installed>,
}

pub fn load_state() -> State {
    fs::read(state_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &State) -> Result<()> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Installed rows whose folder and manifest still exist. A broken folder is
/// returned with an error string so `doctor` can explain it instead of hiding
/// it or crashing the app.
#[derive(Debug, Clone)]
pub struct Entry {
    pub installed: Option<Installed>,
    pub manifest: Result<Manifest, String>,
    pub path: PathBuf,
}

pub fn entries() -> Vec<Entry> {
    let state = load_state();
    let dir = plugins_dir();
    let Ok(read) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name != "data" && !name.starts_with('.'))
        .collect();
    names.sort();
    for name in state.plugins.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let path = plugin_dir(&name);
            let manifest = Manifest::load(&path).map_err(|err| format!("{err:#}"));
            Entry {
                installed: state.plugins.get(&name).cloned(),
                manifest,
                path,
            }
        })
        .collect()
}

pub fn entry(name: &str) -> Option<Entry> {
    if !valid_name(name) {
        return None;
    }
    let path = plugin_dir(name);
    if !path.is_dir() {
        return None;
    }
    let state = load_state();
    Some(Entry {
        installed: state.plugins.get(name).cloned(),
        manifest: Manifest::load(&path).map_err(|err| format!("{err:#}")),
        path,
    })
}

pub fn callable(entry: &Entry) -> Result<(Manifest, Installed)> {
    let manifest = entry
        .manifest
        .clone()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let installed = entry
        .installed
        .clone()
        .ok_or_else(|| anyhow::anyhow!("plugin '{}' is not installed", manifest.name))?;
    if !installed.enabled {
        bail!("plugin '{}' is disabled", manifest.name);
    }
    let missing: Vec<String> = manifest
        .permissions
        .requested()
        .into_iter()
        .filter(|key| !installed.grants.get(key).copied().unwrap_or(false))
        .collect();
    if !missing.is_empty() {
        bail!(
            "plugin '{}' has revoked permission(s): {}",
            manifest.name,
            missing.join(", ")
        );
    }
    if manifest.name != installed.name {
        bail!(
            "manifest name '{}' does not match installed name '{}'",
            manifest.name,
            installed.name
        );
    }
    Ok((manifest, installed))
}

fn atomic_dir_swap(staging: &Path, dest: &Path) -> Result<()> {
    let backup = dest.with_extension("old");
    if backup.exists() {
        fs::remove_dir_all(&backup).ok();
    }
    if dest.exists() {
        fs::rename(dest, &backup).with_context(|| format!("move {}", dest.display()))?;
    }
    match fs::rename(staging, dest) {
        Ok(()) => {
            if backup.exists() {
                let _ = fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(err) => {
            if backup.exists() {
                let _ = fs::rename(&backup, dest);
            }
            Err(err).with_context(|| format!("install into {}", dest.display()))
        }
    }
}

fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for item in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let item = item?;
        let meta = fs::symlink_metadata(item.path())?;
        if meta.file_type().is_symlink() {
            bail!("plugin bundles may not contain symlinks ({})", item.path().display());
        }
        let target = dest.join(item.file_name());
        if meta.is_dir() {
            copy_tree(&item.path(), &target)?;
        } else if meta.is_file() {
            fs::copy(item.path(), &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = meta.permissions().mode();
                let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode));
            }
        } else {
            bail!("plugin bundle contains a special file");
        }
    }
    Ok(())
}

/// Deterministic digest of a directory: relative path, length, then bytes.
pub fn directory_sha256(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    walk_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, path) in files {
        let bytes = fs::read(&path)?;
        hasher.update(rel.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn walk_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let meta = fs::symlink_metadata(item.path())?;
        if meta.file_type().is_symlink() {
            bail!("plugin bundles may not contain symlinks");
        }
        if meta.is_dir() {
            walk_files(root, &item.path(), out)?;
        } else if meta.is_file() {
            let rel = item
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, item.path()));
        }
    }
    Ok(())
}

pub fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected.trim().to_ascii_lowercase() {
        bail!("sha256 mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

fn extract_archive(archive: &Path, staging: &Path) -> Result<()> {
    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(archive)
        .output()
        .context("list plugin archive (tar is required)")?;
    if !listing.status.success() {
        bail!(
            "tar could not list the plugin archive: {}",
            String::from_utf8_lossy(&listing.stderr).trim()
        );
    }
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        let line = line.trim();
        if line.starts_with('/') || line.split('/').any(|part| part == "..") {
            bail!("archive entry escapes the plugin folder: {line}");
        }
    }
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(staging)
        .arg("--no-same-owner")
        .status()
        .context("extract plugin archive")?;
    if !status.success() {
        bail!("tar failed to extract the plugin archive");
    }
    Ok(())
}

fn locate_manifest(dir: &Path) -> Result<PathBuf> {
    let direct = dir.join("omnix-plugin.toml");
    if direct.is_file() {
        return Ok(dir.to_path_buf());
    }
    let mut children = fs::read_dir(dir)?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        if child.path().join("omnix-plugin.toml").is_file() {
            return Ok(child.path());
        }
    }
    bail!("bundle has no omnix-plugin.toml");
}

fn install_source(source: &Path, expected_sha: Option<&str>, source_label: &str) -> Result<Value> {
    let temp = std::env::temp_dir().join(format!("omnix-plugin-{}-{}", std::process::id(), now()));
    let (bundle, cleanup) = if source.is_dir() {
        (source.to_path_buf(), false)
    } else {
        let bytes = fs::read(source).with_context(|| format!("read {}", source.display()))?;
        if let Some(expected) = expected_sha {
            verify_sha256(&bytes, expected)?;
        }
        fs::create_dir_all(&temp)?;
        extract_archive(source, &temp)?;
        (temp.clone(), true)
    };
    let root = locate_manifest(&bundle)?;
    let manifest = Manifest::load(&root)?;
    let sha = directory_sha256(&root)?;
    if let Some(expected) = expected_sha {
        // For a directory source the expected value can name the digest of
        // that directory; archives were already checked above.
        if source.is_dir() && sha != expected.trim().to_ascii_lowercase() {
            bail!("sha256 mismatch: expected {expected}, got {sha}");
        }
    }
    let dest = plugin_dir(&manifest.name);
    let staging = plugins_dir().join(format!(".staging-{}-{}", manifest.name, std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging).ok();
    }
    copy_tree(&root, &staging)?;
    // Re-check the copied tree so a changing source cannot slip past validation.
    let copied = Manifest::load(&staging)?;
    if copied.name != manifest.name || copied.version != manifest.version {
        let _ = fs::remove_dir_all(&staging);
        bail!("plugin changed while copying; try again");
    }
    atomic_dir_swap(&staging, &dest)?;
    let mut state = load_state();
    let mut grants = BTreeMap::new();
    for key in manifest.permissions.requested() {
        grants.insert(key, true);
    }
    let installed = Installed {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        sha256: sha.clone(),
        installed_at: now(),
        source: source_label.to_string(),
        enabled: true,
        grants,
    };
    state.plugins.insert(manifest.name.clone(), installed.clone());
    save_state(&state)?;
    if cleanup {
        let _ = fs::remove_dir_all(&temp);
    }
    Ok(json!({
        "ok": true,
        "name": manifest.name,
        "version": manifest.version,
        "sha256": sha,
        "path": dest.to_string_lossy(),
        "permissions": manifest.permissions.describe(),
        "kinds": kinds(&manifest),
        "note": "installed but not started; it runs only when you call it",
    }))
}

pub fn kinds(manifest: &Manifest) -> Vec<String> {
    let mut out = Vec::new();
    for contribution in &manifest.contributions {
        let kind = match contribution {
            Contribution::Command { .. } => "command",
            Contribution::Agent { .. } => "agent",
            Contribution::BrowserTool { .. } => "browser-tool",
            Contribution::Hook { .. } => "hook",
            Contribution::Panel { .. } => "panel",
        };
        if !out.iter().any(|existing| existing == kind) {
            out.push(kind.to_string());
        }
    }
    out
}

fn catalog_url() -> String {
    std::env::var("OMNIX_PLUGIN_CATALOG").unwrap_or_else(|_| PUBLIC_CATALOG.to_string())
}

fn read_url(url: &str) -> Result<String> {
    if let Some(path) = url.strip_prefix("file://") {
        if std::env::var_os("OMNIX_PLUGIN_CATALOG").is_none() {
            bail!("file:// catalogs are only allowed through OMNIX_PLUGIN_CATALOG");
        }
        return Ok(fs::read_to_string(path).with_context(|| format!("read {path}"))?);
    }
    if !url.starts_with("https://") {
        bail!("refusing to fetch a non-https catalog: {url}");
    }
    let response = ureq::get(url)
        .set("User-Agent", "omnix-plugin")
        .timeout(Duration::from_secs(20))
        .call()
        .with_context(|| format!("GET {url}"))?;
    response.into_string().context("read the plugin catalog")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default = "api_default")]
    pub api: u32,
    #[serde(default)]
    pub plugins: Vec<CatalogEntry>,
}

fn api_default() -> u32 {
    API_VERSION
}

pub fn catalog() -> Result<Catalog> {
    let url = catalog_url();
    let text = read_url(&url)?;
    let catalog: Catalog = serde_json::from_str(&text).context("parse the plugin catalog")?;
    if catalog.api != API_VERSION {
        bail!(
            "catalog API {} is not supported (this Omnix speaks API {API_VERSION})",
            catalog.api
        );
    }
    Ok(catalog)
}

pub fn search(query: &str) -> Result<Value> {
    let catalog = catalog()?;
    let query = query.trim().to_ascii_lowercase();
    let state = load_state();
    let rows: Vec<Value> = catalog
        .plugins
        .into_iter()
        .filter(|entry| {
            query.is_empty()
                || entry.name.to_ascii_lowercase().contains(&query)
                || entry.description.to_ascii_lowercase().contains(&query)
        })
        .map(|entry| {
            json!({
                "name": entry.name,
                "version": entry.version,
                "author": entry.author,
                "license": entry.license,
                "description": entry.description,
                "kinds": entry.kinds,
                "permissions": entry.permissions.describe(),
                "sha256": entry.sha256,
                "installed": state.plugins.get(&entry.name).map(|p| p.version.clone()),
            })
        })
        .collect();
    Ok(json!({ "catalog": catalog_url(), "plugins": rows }))
}

fn download_catalog_entry(entry: &CatalogEntry) -> Result<Value> {
    if !entry.url.starts_with("https://") {
        bail!("refusing to download a plugin over non-https: {}", entry.url);
    }
    let response = ureq::get(&entry.url)
        .set("User-Agent", "omnix-plugin")
        .timeout(Duration::from_secs(120))
        .call()
        .with_context(|| format!("GET {}", entry.url))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(50 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .context("read the plugin download")?;
    if bytes.is_empty() {
        bail!("empty plugin download");
    }
    verify_sha256(&bytes, &entry.sha256)?;
    let dir = std::env::temp_dir().join(format!("omnix-plugin-dl-{}-{}", std::process::id(), now()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("plugin.tar.gz");
    fs::write(&path, &bytes)?;
    let result = install_source(&path, Some(&entry.sha256), &format!("catalog:{}", entry.name));
    let _ = fs::remove_dir_all(&dir);
    result
}

pub fn install(source: &str) -> Result<Value> {
    let path = Path::new(source);
    if path.exists() {
        return install_source(path, None, "local");
    }
    if source.starts_with("https://") {
        bail!("install expects a catalog name, a local folder, or a local .tar.gz");
    }
    let catalog = catalog()?;
    let entry = catalog
        .plugins
        .into_iter()
        .find(|entry| entry.name == source)
        .ok_or_else(|| anyhow::anyhow!("plugin '{source}' is not in the catalog"))?;
    download_catalog_entry(&entry)
}

pub fn update(names: &[String]) -> Result<Value> {
    let catalog = catalog()?;
    let state = load_state();
    let targets: Vec<String> = if names.is_empty() {
        state.plugins.keys().cloned().collect()
    } else {
        names.to_vec()
    };
    let mut rows = Vec::new();
    for name in targets {
        let Some(entry) = catalog.plugins.iter().find(|entry| entry.name == name) else {
            rows.push(json!({"name": name, "status": "not-in-catalog"}));
            continue;
        };
        let current = state.plugins.get(&name).map(|p| p.version.clone());
        if current.as_deref() == Some(entry.version.as_str()) {
            rows.push(json!({"name": name, "status": "up-to-date", "version": entry.version}));
            continue;
        }
        match download_catalog_entry(entry) {
            Ok(value) => rows.push(json!({
                "name": name,
                "status": "updated",
                "version": entry.version,
                "sha256": value["sha256"],
            })),
            Err(err) => rows.push(json!({
                "name": name,
                "status": "failed",
                "error": format!("{err:#}"),
            })),
        }
    }
    Ok(json!({ "plugins": rows }))
}

pub fn remove(name: &str) -> Result<Value> {
    let dir = plugin_dir(name);
    let mut state = load_state();
    if !dir.exists() && !state.plugins.contains_key(name) {
        bail!("plugin '{name}' is not installed");
    }
    if dir.exists() {
        let trash = plugins_dir().join(format!(".removed-{name}-{}", now()));
        fs::rename(&dir, &trash).with_context(|| format!("remove {}", dir.display()))?;
        let _ = fs::remove_dir_all(&trash);
    }
    state.plugins.remove(name);
    save_state(&state)?;
    let _ = fs::remove_dir_all(data_dir(name));
    Ok(json!({"ok": true, "name": name, "removed": true}))
}

pub fn set_enabled(name: &str, enabled: bool) -> Result<Value> {
    let mut state = load_state();
    let installed = state
        .plugins
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not installed"))?;
    installed.enabled = enabled;
    save_state(&state)?;
    Ok(json!({"ok": true, "name": name, "enabled": enabled}))
}

pub fn revoke(name: &str, keys: &[String]) -> Result<Value> {
    let mut state = load_state();
    let installed = state
        .plugins
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not installed"))?;
    for key in keys {
        if !matches!(key.as_str(), "read" | "write" | "network" | "agents" | "history") {
            bail!("unknown permission '{key}'");
        }
        if !installed.grants.contains_key(key) {
            bail!("plugin '{name}' did not request '{key}'");
        }
        installed.grants.insert(key.clone(), false);
    }
    let grants = installed.grants.clone();
    save_state(&state)?;
    Ok(json!({"ok": true, "name": name, "grants": grants}))
}

pub fn grant(name: &str, keys: &[String]) -> Result<Value> {
    let mut state = load_state();
    let installed = state
        .plugins
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not installed"))?;
    for key in keys {
        if !matches!(key.as_str(), "read" | "write" | "network" | "agents" | "history") {
            bail!("unknown permission '{key}'");
        }
        if !installed.grants.contains_key(key) {
            bail!("plugin '{name}' did not request '{key}'");
        }
        installed.grants.insert(key.clone(), true);
    }
    let grants = installed.grants.clone();
    save_state(&state)?;
    Ok(json!({"ok": true, "name": name, "grants": grants}))
}

pub fn list() -> Vec<Value> {
    entries()
        .into_iter()
        .map(|entry| {
            let manifest = entry.manifest.as_ref().ok();
            let installed = entry.installed.as_ref();
            let granted = installed
                .map(|p| {
                    p.grants
                        .iter()
                        .filter(|(_, value)| **value)
                        .map(|(key, _)| key.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let requested = manifest
                .map(|m| m.permissions.requested().into_iter().collect::<Vec<_>>())
                .unwrap_or_default();
            json!({
                "name": installed.map(|p| p.name.clone()).unwrap_or_else(|| entry.path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string()),
                "version": installed.map(|p| p.version.clone()),
                "enabled": installed.map(|p| p.enabled).unwrap_or(true),
                "installed": installed.is_some(),
                "path": entry.path.to_string_lossy(),
                "kinds": manifest.map(kinds).unwrap_or_default(),
                "permissions": manifest.map(|m| m.permissions.describe()).unwrap_or_else(|| "unreadable".into()),
                "requested": requested,
                "granted": granted,
                "description": manifest.map(|m| m.description.clone()),
                "author": manifest.map(|m| m.author.clone()),
                "license": manifest.map(|m| m.license.clone()),
                "api": manifest.map(|m| m.api),
                "sha256": installed.map(|p| p.sha256.clone()),
                "source": installed.map(|p| p.source.clone()),
                "error": entry.manifest.err(),
            })
        })
        .collect()
}

pub fn doctor() -> Value {
    let state = load_state();
    let mut rows = Vec::new();
    let mut healthy = true;
    for entry in entries() {
        let name = entry
            .installed
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                entry
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string()
            });
        let mut problems = Vec::new();
        match &entry.manifest {
            Ok(manifest) => {
                if let Err(err) = resolve_entry(&entry.path, &manifest.runtime.entry) {
                    problems.push(format!("{err:#}"));
                }
                if let Some(installed) = &entry.installed {
                    match directory_sha256(&entry.path) {
                        Ok(sha) if sha != installed.sha256 => {
                            problems.push("files changed since install".into());
                        }
                        Err(err) => problems.push(format!("{err:#}")),
                        _ => {}
                    }
                }
            }
            Err(err) => problems.push(err.clone()),
        }
        if entry.installed.is_none() {
            problems.push("folder exists but state.json does not list it".into());
        }
        if !entry.path.is_dir() {
            problems.push("plugin folder is missing".into());
        }
        if !problems.is_empty() {
            healthy = false;
        }
        rows.push(json!({
            "name": name,
            "ok": problems.is_empty(),
            "problems": problems,
            "path": entry.path.to_string_lossy(),
        }));
    }
    for name in state.plugins.keys() {
        if !plugin_dir(name).is_dir() {
            healthy = false;
            rows.push(json!({
                "name": name,
                "ok": false,
                "problems": ["installed state has no folder"],
            }));
        }
    }
    json!({
        "ok": healthy,
        "api": API_VERSION,
        "plugins": rows,
        "path": plugins_dir().to_string_lossy(),
    })
}

fn resolve_entry(dir: &Path, entry: &[String]) -> Result<PathBuf> {
    let first = entry.first().context("runtime.entry is empty")?;
    let candidate = Path::new(first);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        if !safe_rel(first) {
            bail!("plugin entry escapes its folder");
        }
        let path = dir.join(first);
        if !path.is_file() {
            bail!("entry not found: {}", path.display());
        }
        return Ok(path);
    }
    Ok(PathBuf::from(first))
}

fn allowed_env() -> &'static [&'static str] {
    &[
        "PATH",
        "HOME",
        "USERPROFILE",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TZ",
        "TMPDIR",
        "TEMP",
        "TMP",
        "SystemRoot",
        "WINDIR",
        "PATHEXT",
        "COMSPEC",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_RUNTIME_DIR",
    ]
}

fn decode_timeout(manifest: &Manifest) -> Duration {
    let env = std::env::var("OMNIX_PLUGIN_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    let ms = env
        .or(manifest.runtime.timeout_ms)
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(250, MAX_TIMEOUT_MS);
    Duration::from_millis(ms)
}

fn decode_ram(manifest: &Manifest) -> u64 {
    std::env::var("OMNIX_PLUGIN_RAM_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or(manifest.runtime.ram_mb)
        .unwrap_or(DEFAULT_RAM_MB)
        .clamp(32, 4096)
}

#[cfg(unix)]
fn limit_child(command: &mut Command, ram_mb: u64) {
    use std::os::unix::process::CommandExt;
    let bytes = ram_mb.saturating_mul(1024 * 1024);
    // SAFETY: setrlimit is async-signal-safe and touches only the child after
    // fork; no allocation or Rust runtime work happens here.
    unsafe {
        command.pre_exec(move || {
            let limit = libc::rlimit {
                rlim_cur: bytes as libc::rlim_t,
                rlim_max: bytes as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn limit_child(_command: &mut Command, _ram_mb: u64) {}

fn spawn_request(
    dir: &Path,
    entry: &[String],
    env: &BTreeMap<String, String>,
    request: &Value,
    timeout: Duration,
    ram_mb: u64,
) -> Result<Value> {
    let program = resolve_entry(dir, entry)?;
    let mut command = Command::new(&program);
    if program.components().count() > 1 || program.is_absolute() {
        command.args(&entry[1..]);
    } else {
        command.args(&entry[1..]);
    }
    command.current_dir(dir);
    command.env_clear();
    for key in allowed_env() {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    limit_child(&mut command, ram_mb);
    let mut child = command
        .spawn()
        .with_context(|| format!("start plugin entry {}", program.display()))?;
    let mut stdin = child.stdin.take().context("plugin stdin")?;
    let mut body = serde_json::to_vec(request)?;
    body.push(b'\n');
    if let Err(err) = stdin.write_all(&body).and_then(|_| stdin.flush()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err).context("send request to plugin");
    }
    drop(stdin);
    let stdout = child.stdout.take().context("plugin stdout")?;
    let stderr = child.stderr.take().context("plugin stderr")?;
    let (tx, rx) = mpsc::channel::<std::result::Result<String, String>>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout.take(MAX_RESPONSE));
        let mut line = String::new();
        let result = reader
            .read_line(&mut line)
            .map(|_| line)
            .map_err(|err| err.to_string());
        let _ = tx.send(result);
    });
    let (err_tx, err_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr.take(64 * 1024)).read_to_string(&mut text);
        let _ = err_tx.send(text);
    });
    let reply = match rx.recv_timeout(timeout) {
        Ok(Ok(line)) => line,
        Ok(Err(err)) => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("read plugin response: {err}");
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let tail = err_rx.recv_timeout(Duration::from_millis(200)).unwrap_or_default();
            bail!(
                "plugin '{}' timed out after {} ms{}",
                dir.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                timeout.as_millis(),
                if tail.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", tail.trim().chars().take(500).collect::<String>())
                }
            );
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    if reply.trim().is_empty() {
        let tail = err_rx.recv_timeout(Duration::from_millis(200)).unwrap_or_default();
        bail!(
            "plugin exited without a response{}",
            if tail.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", tail.trim().chars().take(500).collect::<String>())
            }
        );
    }
    let reply: Value = serde_json::from_str(reply.trim()).context("plugin reply is not JSON")?;
    if let Some(error) = reply.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("plugin call failed");
        bail!("{message}");
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// Invoke one installed plugin. The process starts here and exits when the
/// answer is read; Omnix never keeps a plugin server alive.
pub fn invoke(name: &str, method: &str, params: Value) -> Result<Value> {
    if !valid_name(name) {
        bail!("invalid plugin name");
    }
    let entry = entry(name).ok_or_else(|| anyhow::anyhow!("plugin '{name}' is not installed"))?;
    let (manifest, installed) = callable(&entry)?;
    let timeout = decode_timeout(&manifest);
    let ram = decode_ram(&manifest);
    let permissions = installed
        .grants
        .iter()
        .filter(|(_, value)| **value)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>()
        .join(",");
    let mut env = BTreeMap::new();
    env.insert("OMNIX_PLUGIN_NAME".into(), manifest.name.clone());
    env.insert("OMNIX_PLUGIN_API".into(), API_VERSION.to_string());
    env.insert("OMNIX_PLUGIN_DIR".into(), entry.path.to_string_lossy().into_owned());
    env.insert("OMNIX_PLUGIN_PERMISSIONS".into(), permissions);
    env.insert(
        "OMNIX_PLUGIN_DATA".into(),
        data_dir(&manifest.name).to_string_lossy().into_owned(),
    );
    let _ = fs::create_dir_all(data_dir(&manifest.name));
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    spawn_request(
        &entry.path,
        &manifest.runtime.entry,
        &env,
        &request,
        timeout,
        ram,
    )
}

pub fn run_command(name: &str, args: &[String]) -> Result<Value> {
    invoke(
        name,
        "command",
        json!({"plugin": name, "args": args, "cwd": std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned())}),
    )
}

pub fn command_names() -> Vec<(String, String)> {
    entries()
        .into_iter()
        .filter_map(|entry| {
            let manifest = entry.manifest.ok()?;
            let installed = entry.installed?;
            if !installed.enabled {
                return None;
            }
            let commands = manifest.commands();
            Some(
                commands
                    .into_iter()
                    .map(|(name, title)| (format!("{}:{}", manifest.name, name), title.to_string()))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

/// Split `plugin:command` for the CLI. A bare plugin name is also accepted
/// when that plugin has exactly one command contribution.
pub fn resolve_command(name: &str) -> Option<(String, String)> {
    if let Some((plugin, command)) = name.split_once(':') {
        let entry = entry(plugin)?;
        let manifest = entry.manifest.ok()?;
        if manifest
            .commands()
            .iter()
            .any(|(candidate, _)| *candidate == command)
        {
            return Some((plugin.to_string(), command.to_string()));
        }
        return None;
    }
    let entry = entry(name)?;
    let manifest = entry.manifest.ok()?;
    let commands = manifest.commands();
    if commands.len() == 1 {
        Some((name.to_string(), commands[0].0.to_string()))
    } else {
        None
    }
}

/// Hook plugins run on a detached thread after an event has been appended.
/// With no plugin folder this is one `read_dir` that fails, then nothing.
pub fn dispatch_hooks(kind: &str, event: &Value) {
    let entries = entries();
    if entries.is_empty() {
        return;
    }
    let mut started = 0;
    for entry in entries {
        if started >= 4 {
            break;
        }
        let Ok((manifest, _installed)) = callable(&entry) else {
            continue;
        };
        let matches = manifest.hooks().iter().any(|hook| {
            if let Contribution::Hook { events } = hook {
                events.iter().any(|want| want == "*" || want == kind)
            } else {
                false
            }
        });
        if !matches {
            continue;
        }
        started += 1;
        let name = manifest.name.clone();
        let payload = event.clone();
        let event_kind = kind.to_string();
        std::thread::spawn(move || {
            let _ = invoke(
                &name,
                "hook",
                json!({"event": payload, "name": event_kind, "at": now()}),
            );
        });
    }
}

pub fn browser_tools() -> Vec<Value> {
    let mut rows = Vec::new();
    for entry in entries() {
        let Ok((manifest, _)) = callable(&entry) else {
            continue;
        };
        for tool in manifest.browser_tools() {
            if let Contribution::BrowserTool { name, description, input_schema } = tool {
                rows.push(json!({
                    "name": format!("plugin__{}", name),
                    "plugin": manifest.name,
                    "tool": name,
                    "description": description,
                    "inputSchema": input_schema.clone().unwrap_or_else(|| json!({"type":"object","additionalProperties":true})),
                }));
            }
        }
    }
    rows
}

pub fn call_browser_tool(plugin: &str, tool: &str, arguments: Value, endpoint: Option<&str>) -> Result<Value> {
    let entry = entry(plugin).ok_or_else(|| anyhow::anyhow!("plugin '{plugin}' is not installed"))?;
    let (manifest, _) = callable(&entry)?;
    if !manifest.browser_tools().iter().any(|contribution| {
        matches!(contribution, Contribution::BrowserTool { name, .. } if name == tool)
    }) {
        bail!("plugin '{plugin}' does not provide browser tool '{tool}'");
    }
    invoke(
        plugin,
        "browser-tool",
        json!({
            "plugin": plugin,
            "tool": tool,
            "arguments": arguments,
            "browser": { "endpoint": endpoint, "engine": "lightpanda" },
        }),
    )
}

pub fn agent_contribution(name: &str) -> Option<(String, String)> {
    for entry in entries() {
        let manifest = entry.manifest.ok()?;
        for contribution in manifest.agents() {
            if let Contribution::Agent { name: agent, bin, args, env, resume } = contribution {
                if agent == name {
                    return Some((
                        manifest.name.clone(),
                        serde_json::to_string(&json!({
                            "bin": bin,
                            "args": args,
                            "env": env,
                            "resume": resume,
                        }))
                        .ok()?,
                    ));
                }
            }
        }
    }
    None
}

/// The launch binary for an agent contributed by a plugin. The GUI and the
/// handoff path only need a command name; args/env live in `plugin_agent`.
pub fn agent_bin(name: &str) -> Option<String> {
    for entry in entries() {
        let Ok((manifest, _)) = callable(&entry) else {
            continue;
        };
        for contribution in manifest.agents() {
            if let Contribution::Agent { name: agent, bin, .. } = contribution {
                if agent == name {
                    return Some(bin.clone());
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Default)]
pub struct PluginAgent {
    pub bin: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub resume: Vec<String>,
    pub dir: PathBuf,
}

pub fn agent_profile(name: &str) -> Option<PluginAgent> {
    for entry in entries() {
        let (manifest, _) = callable(&entry).ok()?;
        for contribution in manifest.agents() {
            if let Contribution::Agent { name: agent, bin, args, env, resume } = contribution {
                if agent == name {
                    return Some(PluginAgent {
                        bin: bin.clone(),
                        args: args.clone(),
                        env: env.clone(),
                        resume: resume.clone(),
                        dir: entry.path.clone(),
                    });
                }
            }
        }
    }
    None
}

pub fn is_agent(name: &str) -> bool {
    agent_bin(name).is_some()
}

/// Rows for Settings / Agents. A plugin agent is shown only while the plugin is
/// enabled and its permissions are intact; the launch path is validated again
/// when the pane actually opens.
pub fn agent_rows() -> Vec<Value> {
    let mut rows = Vec::new();
    for entry in entries() {
        let Ok((manifest, _)) = callable(&entry) else {
            continue;
        };
        for contribution in manifest.agents() {
            if let Contribution::Agent { name, bin, args, env, .. } = contribution {
                let path = PathBuf::from(bin);
                let available = if path.is_absolute() {
                    path.is_file()
                } else if bin.contains('/') || bin.contains('\\') {
                    entry.path.join(&path).is_file()
                } else {
                    crate::agent::resolve(name).is_some()
                };
                rows.push(json!({
                    "name": name,
                    "bin": bin,
                    "available": available,
                    "enabled": true,
                    "args": args.join(" "),
                    "env": env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "),
                    "plugin": manifest.name,
                    "fullAccessFlag": null,
                }));
            }
        }
    }
    rows
}

pub fn resume_args(name: &str, session: &str) -> Option<Vec<String>> {
    let agent = agent_profile(name)?;
    if agent.resume.is_empty() {
        return None;
    }
    Some(
        agent
            .resume
            .iter()
            .map(|part| part.replace("{session}", session))
            .collect(),
    )
}

pub fn panel_list() -> Vec<Value> {
    let mut rows = Vec::new();
    for entry in entries() {
        let Ok((manifest, _)) = callable(&entry) else {
            continue;
        };
        for (name, title, _) in manifest.panels() {
            rows.push(json!({
                "plugin": manifest.name,
                "name": name,
                "title": if title.is_empty() { name } else { title },
            }));
        }
    }
    rows
}

pub fn panel_info(plugin: &str, panel: Option<&str>) -> Result<Value> {
    let entry = entry(plugin).ok_or_else(|| anyhow::anyhow!("plugin '{plugin}' is not installed"))?;
    let (manifest, _) = callable(&entry)?;
    let panels = manifest.panels();
    let selected = match panel {
        Some(wanted) => panels
            .iter()
            .find(|(name, _, _)| *name == wanted)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("panel '{wanted}' not found"))?,
        None => *panels
            .first()
            .ok_or_else(|| anyhow::anyhow!("plugin '{plugin}' has no panel"))?,
    };
    Ok(json!({
        "plugin": manifest.name,
        "name": selected.0,
        "title": if selected.1.is_empty() { selected.0 } else { selected.1 },
        "url": format!("/plugin-panel/{}/{}", manifest.name, selected.2),
    }))
}

/// Resolve a file requested by a panel. The result must stay under the plugin
/// folder; symlinks are refused.
pub fn panel_file(plugin: &str, rel: &str) -> Result<(PathBuf, String)> {
    let entry = entry(plugin).ok_or_else(|| anyhow::anyhow!("plugin '{plugin}' is not installed"))?;
    let (manifest, _) = callable(&entry)?;
    let panels = manifest.panels();
    if panels.is_empty() {
        bail!("plugin '{plugin}' has no panel");
    }
    if !safe_rel(rel) {
        bail!("invalid panel path");
    }
    let root = fs::canonicalize(&entry.path)?;
    let path = root.join(rel);
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() {
        bail!("panel files may not be symlinks");
    }
    let path = fs::canonicalize(&path)?;
    if !path.starts_with(&root) || !path.is_file() {
        bail!("panel path escapes the plugin");
    }
    let size = fs::metadata(&path)?.len();
    if size > MAX_FILE {
        bail!("panel file is too large");
    }
    let kind = match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    Ok((path, kind.to_string()))
}

/// `omnix plugin new <name> --kind command|panel|hook|agent|browser-tool`.
pub fn scaffold(name: &str, kind: &str, dir: Option<PathBuf>) -> Result<Value> {
    if !valid_name(name) {
        bail!("name must be 1-48 chars of a-z, 0-9, '-'");
    }
    if !matches!(
        kind,
        "command" | "panel" | "hook" | "agent" | "browser-tool"
    ) {
        bail!("--kind must be command, panel, hook, agent, or browser-tool");
    }
    let target = dir.unwrap_or_else(|| PathBuf::from(name));
    if target.exists() {
        bail!("{} already exists", target.display());
    }
    fs::create_dir_all(&target)?;
    let contribution = match kind {
        "command" => "[[contributions]]\nkind = \"command\"\nname = \"NAME\"\ntitle = \"NAME\"\ndescription = \"What this command does\"\n".replace("NAME", name),
        "panel" => format!("[[contributions]]\nkind = \"panel\"\nname = \"{name}\"\ntitle = \"{name}\"\nentry = \"panel.html\"\n"),
        "hook" => "[[contributions]]\nkind = \"hook\"\nevents = [\"handoff-created\"]\n".to_string(),
        "agent" => format!("[[contributions]]\nkind = \"agent\"\nname = \"{name}\"\nbin = \"{name}\"\nargs = []\nresume = [\"--resume\", \"{{session}}\"]\n[contributions.env]\n"),
        "browser-tool" => format!("[[contributions]]\nkind = \"browser-tool\"\nname = \"{name}\"\ndescription = \"A tool on top of the Lightpanda session\"\n[contributions.input_schema]\ntype = \"object\"\n"),
        _ => unreachable!(),
    };
    let manifest = format!(
        "name = \"{name}\"\nversion = \"0.1.0\"\nauthor = \"Your Name\"\nlicense = \"MIT\"\ndescription = \"A small Omnix plugin\"\napi = 1\n\n[runtime]\nentry = [\"python3\", \"plugin.py\"]\ntimeout_ms = 15000\nram_mb = 256\n\n[permissions]\nread = []\nwrite = []\nnetwork = []\nagents = false\nhistory = false\n\n{contribution}"
    );
    fs::write(target.join("omnix-plugin.toml"), manifest)?;
    fs::write(target.join("plugin.py"), SCAFFOLD_RUNTIME)?;
    if kind == "panel" {
        fs::write(
            target.join("panel.html"),
            "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Plugin panel</title>\n<style>body{font:14px system-ui;padding:18px;color:#ddd;background:#18191c}</style>\n<h1>Plugin panel</h1>\n<p id=\"out\">Waiting for the host...</p>\n<script>\nparent.postMessage({omnixPanel:true, ready:true}, '*');\nwindow.addEventListener('message', function (ev) {\n  if (!ev.data || !ev.data.omnixPanelReply) return;\n  document.getElementById('out').textContent = JSON.stringify(ev.data.result);\n});\nparent.postMessage({omnixPanel:true, method:'status', params:{}}, '*');\n</script>\n",
        )?;
    }
    Ok(json!({"ok": true, "path": target.to_string_lossy(), "kind": kind, "next": "read docs/plugin-api.md"}))
}

const SCAFFOLD_RUNTIME: &str = r#"#!/usr/bin/env python3
"""One request per line on stdin, one JSON-RPC response per line on stdout."""
import json
import os
import sys


def main():
    line = sys.stdin.readline()
    if not line:
        return 1
    request = json.loads(line)
    method = request.get("method", "")
    params = request.get("params", {}) or {}
    if method in ("command", "hook", "browser-tool"):
        result = {"stdout": "hello from " + os.environ.get("OMNIX_PLUGIN_NAME", "plugin") + "\n",
                  "method": method, "params": params}
    else:
        result = {"method": method, "ok": True}
    print(json.dumps({"jsonrpc": "2.0", "id": request.get("id", 1), "result": result}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-plugin-{tag}-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest_text(name: &str, extra: &str) -> String {
        format!(
            "name = \"{name}\"\nversion = \"0.1.0\"\nauthor = \"Tester\"\nlicense = \"MIT\"\ndescription = \"test\"\napi = 1\n\n[runtime]\nentry = [\"sh\", \"plugin.sh\"]\ntimeout_ms = 1000\n\n{extra}"
        )
    }

    fn write_plugin(dir: &Path, name: &str, script: &str, extra: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("omnix-plugin.toml"), manifest_text(name, extra)).unwrap();
        fs::write(dir.join("plugin.sh"), script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                dir.join("plugin.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
    }

    #[test]
    fn manifest_parses_kinds_and_permissions() {
        let text = "name = \"hello\"\nversion = \"1.2.3\"\nauthor = \"A\"\nlicense = \"MIT\"\ndescription = \"d\"\napi = 1\n\n[runtime]\nentry = [\"bin/hello\"]\n\n[permissions]\nread = [\"/tmp/example\"]\nnetwork = [\"api.example.com\"]\n\n[[contributions]]\nkind = \"command\"\nname = \"hello\"\n\n[[contributions]]\nkind = \"browser-tool\"\nname = \"lookup\"\ndescription = \"look up\"\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.name, "hello");
        assert_eq!(manifest.commands().len(), 1);
        assert_eq!(manifest.browser_tools().len(), 1);
        assert!(manifest.permissions.describe().contains("network"));
        assert_eq!(manifest.api, API_VERSION);
    }

    #[test]
    fn shipped_examples_have_valid_manifests() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("plugins");
        let hello = Manifest::load(&root.join("hello-command")).unwrap();
        assert_eq!(hello.name, "hello-command");
        assert_eq!(hello.commands()[0].0, "hello");
        let panel = Manifest::load(&root.join("status-panel")).unwrap();
        assert_eq!(panel.name, "status-panel");
        assert_eq!(panel.panels()[0].0, "status");
    }

    #[test]
    fn incompatible_api_and_bad_names_are_refused() {
        let mut text = manifest_text("hello", "");
        assert!(Manifest::parse(&text).is_ok());
        text = text.replace("api = 1", "api = 99");
        assert!(Manifest::parse(&text).unwrap_err().to_string().contains("API"));
        let text = manifest_text("Bad_Name", "");
        assert!(Manifest::parse(&text).is_err());
        let text = manifest_text("hello", "").replace("\"0.1.0\"", "\"one\"");
        assert!(Manifest::parse(&text).is_err());
    }

    #[test]
    fn sha256_matches_and_rejects() {
        let bytes = b"plugin bytes";
        let good = format!("{:x}", Sha256::digest(bytes));
        assert!(verify_sha256(bytes, &good).is_ok());
        assert!(verify_sha256(bytes, "00").is_err());
    }

    #[test]
    fn entry_cannot_escape_the_plugin_folder() {
        let dir = temp("entry");
        assert!(resolve_entry(&dir, &["../../bin/sh".into()]).is_err());
        assert!(resolve_entry(&dir, &["/bin/sh".into()]).is_err());
        assert!(resolve_entry(&dir, &["plugin.sh".into()]).is_ok());
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn install_run_disable_and_remove() {
        let _guard = env_lock();
        let home = temp("run-home");
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        let src = temp("run-src");
        write_plugin(
            &src,
            "echoer",
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"stdout\":\"ok\"}}\\n'\n",
            "[[contributions]]\nkind = \"command\"\nname = \"echoer\"\n",
        );
        let value = install_source(&src, None, "test").unwrap();
        assert_eq!(value["name"], "echoer");
        let result = invoke("echoer", "command", json!({"args": []})).unwrap();
        assert_eq!(result["stdout"], "ok");
        set_enabled("echoer", false).unwrap();
        assert!(invoke("echoer", "command", json!({})).is_err());
        set_enabled("echoer", true).unwrap();
        remove("echoer").unwrap();
        assert!(!plugin_dir("echoer").exists());
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&src).ok();
        unsafe { std::env::remove_var("OMNIX_HOME") };
    }

    #[cfg(unix)]
    #[test]
    fn revoked_permission_refuses_the_call() {
        let _guard = env_lock();
        let home = temp("revoke-home");
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        let src = temp("revoke-src");
        write_plugin(
            &src,
            "reader",
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\\n'\n",
            "[permissions]\nread = [\"/tmp/data\"]\n\n[[contributions]]\nkind = \"command\"\nname = \"reader\"\n",
        );
        install_source(&src, None, "test").unwrap();
        assert!(invoke("reader", "command", json!({})).is_ok());
        revoke("reader", &["read".into()]).unwrap();
        let error = invoke("reader", "command", json!({})).unwrap_err().to_string();
        assert!(error.contains("revoked"));
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&src).ok();
        unsafe { std::env::remove_var("OMNIX_HOME") };
    }

    #[cfg(unix)]
    #[test]
    fn crash_and_timeout_do_not_panic() {
        let _guard = env_lock();
        let home = temp("crash-home");
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        unsafe { std::env::set_var("OMNIX_PLUGIN_TIMEOUT_MS", "1000") };
        let src = temp("crash-src");
        write_plugin(
            &src,
            "crasher",
            "#!/bin/sh\necho boom >&2\nexit 7\n",
            "[[contributions]]\nkind = \"command\"\nname = \"crasher\"\n",
        );
        install_source(&src, None, "test").unwrap();
        let error = invoke("crasher", "command", json!({})).unwrap_err().to_string();
        assert!(error.contains("without a response") || error.contains("boom"));
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&src).ok();
        unsafe { std::env::remove_var("OMNIX_HOME") };

        let home = temp("slow-home");
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        let src = temp("slow-src");
        write_plugin(
            &src,
            "slow",
            "#!/bin/sh\nsleep 5\n",
            "[[contributions]]\nkind = \"command\"\nname = \"slow\"\n",
        );
        install_source(&src, None, "test").unwrap();
        let started = std::time::Instant::now();
        let error = invoke("slow", "command", json!({})).unwrap_err().to_string();
        assert!(error.contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(4));
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&src).ok();
        unsafe { std::env::remove_var("OMNIX_HOME") };
        unsafe { std::env::remove_var("OMNIX_PLUGIN_TIMEOUT_MS") };
    }

    #[test]
    fn scaffold_writes_a_valid_manifest() {
        let dir = temp("scaffold");
        let target = dir.join("my-plugin");
        scaffold("my-plugin", "command", Some(target.clone())).unwrap();
        let manifest = Manifest::load(&target).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.commands().len(), 1);
        fs::remove_dir_all(&dir).ok();
    }
}
