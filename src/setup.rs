//! First-run setup.
//!
//! The wizard is a one-time convenience, not a gate. State lives in
//! `.omnix/setup.json`; an existing project list also suppresses the wizard so
//! people upgrading from an older Omnix are not interrupted.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SUPPORT_URL: &str = "https://tako.id/Zwart04/gift";

/// Open the fixed support page in the user's default browser. The URL is a
/// constant, so this RPC cannot be turned into an arbitrary launcher.
pub fn open_support() -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut cmd = std::process::Command::new("open");
        cmd.arg(SUPPORT_URL);
        cmd
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "", SUPPORT_URL]);
        cmd
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(SUPPORT_URL);
        cmd
    };
    cmd.spawn().context("open the support page")?;
    Ok(())
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub completed_at: u64,
    #[serde(default)]
    pub version: String,
}

pub fn store_path(root: &Path) -> PathBuf {
    root.join(".omnix").join("setup.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load(root: &Path) -> State {
    fs::read(store_path(root))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Show the wizard only on a genuinely new workspace. A saved project means
/// the person already chose where they work, even if this file is missing.
pub fn needed(root: &Path) -> bool {
    !store_path(root).exists() && crate::projects::load(root).is_empty()
}

pub fn payload(root: &Path) -> Value {
    let state = load(root);
    json!({
        "needed": needed(root),
        "complete": state.completed_at > 0,
        "completedAt": state.completed_at,
        "version": state.version,
        "path": store_path(root).to_string_lossy(),
        "supportUrl": SUPPORT_URL,
    })
}

pub fn complete(root: &Path) -> Result<State> {
    let path = store_path(root);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let state = State {
        completed_at: now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    fs::write(&path, serde_json::to_vec_pretty(&state)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(state)
}

pub fn reset(root: &Path) -> Result<()> {
    match fs::remove_file(store_path(root)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove {}", store_path(root).display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "omnix-setup-{}-{}-{nanos}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn fresh_workspace_needs_setup_once() {
        let root = temp_root("fresh");
        assert!(needed(&root));
        assert!(!payload(&root)["complete"].as_bool().unwrap());

        let state = complete(&root).unwrap();
        assert!(state.completed_at > 0);
        assert_eq!(state.version, env!("CARGO_PKG_VERSION"));
        assert!(!needed(&root));
        assert!(payload(&root)["complete"].as_bool().unwrap());

        reset(&root).unwrap();
        assert!(needed(&root));
        assert!(!store_path(&root).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_project_suppresses_the_wizard() {
        let root = temp_root("project");
        let project = root.join("work");
        fs::create_dir_all(&project).unwrap();
        crate::projects::add(&root, project.to_str().unwrap()).unwrap();

        assert!(!needed(&root));
        assert!(!payload(&root)["needed"].as_bool().unwrap());

        // resetting the record still leaves existing users alone
        complete(&root).unwrap();
        reset(&root).unwrap();
        assert!(!needed(&root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_state_never_reopens_the_wizard() {
        let root = temp_root("corrupt");
        fs::create_dir_all(root.join(".omnix")).unwrap();
        fs::write(store_path(&root), b"not json").unwrap();

        assert!(!needed(&root));
        assert!(!payload(&root)["complete"].as_bool().unwrap());

        // completing again repairs the file
        complete(&root).unwrap();
        assert!(payload(&root)["complete"].as_bool().unwrap());
        let _ = fs::remove_dir_all(&root);
    }
}
