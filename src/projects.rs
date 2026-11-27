use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A project the user added to the sidebar. Orca shows the workspaces you
/// opened, not every folder that happens to have agent transcripts, so this
/// list is the source of truth and indexed history only enriches it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub path: String,
    #[serde(default)]
    pub name: String,
}

pub fn store_path(root: &Path) -> PathBuf {
    root.join(".omnix").join("projects.json")
}

pub fn load(root: &Path) -> Vec<Project> {
    std::fs::read(store_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(root: &Path, list: &[Project]) -> Result<()> {
    let path = store_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(list)?)?;
    Ok(())
}

/// Canonicalize so `/repo` and `/repo/` cannot both be added.
fn canonical(path: &str) -> Result<PathBuf> {
    let given = PathBuf::from(path.trim());
    if !given.is_absolute() {
        bail!("project path must be absolute");
    }
    let real = std::fs::canonicalize(&given)
        .map_err(|err| anyhow::anyhow!("cannot open {}: {err}", given.display()))?;
    if !real.is_dir() {
        bail!("not a directory: {}", real.display());
    }
    Ok(real)
}

pub fn add(root: &Path, path: &str) -> Result<Vec<Project>> {
    let real = canonical(path)?;
    let entry = Project {
        name: real
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| real.to_string_lossy().into_owned()),
        path: real.to_string_lossy().into_owned(),
    };
    let mut list = load(root);
    if !list.iter().any(|p| p.path == entry.path) {
        list.push(entry);
        save(root, &list)?;
    }
    Ok(list)
}

pub fn remove(root: &Path, path: &str) -> Result<Vec<Project>> {
    let mut list = load(root);
    list.retain(|p| p.path != path);
    save(root, &list)?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("omnix-proj-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn add_dedupes_and_remove_drops() {
        let root = temp_root("add");
        let dir = root.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_string_lossy().into_owned();

        let list = add(&root, &format!("{path}/")).unwrap();
        assert_eq!(list.len(), 1, "trailing slash is the same project");
        assert_eq!(list[0].name, "repo");

        let list = add(&root, &path).unwrap();
        assert_eq!(list.len(), 1, "second add is a no-op");

        let list = remove(&root, &list[0].path).unwrap();
        assert!(list.is_empty(), "remove drops it from the store");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_relative_and_missing_paths() {
        let root = temp_root("reject");
        assert!(add(&root, "relative/dir").is_err());
        assert!(add(&root, "/definitely/missing/omnix-project").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
