//! Cooperative, on-demand leases for shared resources.
//!
//! A lock is one small JSON file under `<repo>/.omnix/locks/`. There is no
//! daemon and no timer: every read checks the TTL and whether the recorded PID
//! is still alive, so an abandoned lock expires on its own. Locks are advisory
//! and only protect resources Omnix itself starts (worktrees, branches, preview
//! ports, declared paths).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_TTL: u64 = 4 * 60 * 60;
const MAX_TTL: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lock {
    pub id: String,
    pub resource: String,
    pub owner: String,
    pub pid: u32,
    pub created_at: u64,
    pub expires_at: u64,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn locks_dir(root: &Path) -> PathBuf {
    root.join(".omnix").join("locks")
}

fn valid_resource(resource: &str) -> bool {
    let trimmed = resource.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 200
        && !trimmed.chars().any(char::is_control)
}

fn normalize(resource: &str) -> Result<String> {
    let resource = resource.trim();
    if !valid_resource(resource) {
        bail!("lock resource must be 1-200 printable characters");
    }
    Ok(resource.to_string())
}

fn filename(resource: &str) -> String {
    let mut safe = String::new();
    for ch in resource.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            safe.push(ch.to_ascii_lowercase());
        } else if !safe.ends_with('-') {
            safe.push('-');
        }
        if safe.len() >= 40 {
            break;
        }
    }
    let digest = format!("{:x}", Sha256::digest(resource.as_bytes()));
    format!("{}-{}.json", safe.trim_matches('-'), &digest[..12])
}

fn path_for(root: &Path, resource: &str) -> PathBuf {
    locks_dir(root).join(filename(resource))
}

fn id_for(resource: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(resource.as_bytes()));
    digest[..16].to_string()
}

fn read(path: &Path) -> Option<Lock> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write(path: &Path, lock: &Lock) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = fs::File::create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    file.write_all(serde_json::to_vec_pretty(lock)?.as_slice())?;
    Ok(())
}

fn write_new(path: &Path, lock: &Lock) -> Result<bool> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(err) => return Err(err).with_context(|| format!("create {}", path.display())),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    file.write_all(serde_json::to_vec_pretty(lock)?.as_slice())?;
    Ok(true)
}

/// A lock is active only while both its lease and its owner live. An expired
/// lock or one whose PID is gone is stale and may be replaced.
pub fn active(lock: &Lock) -> bool {
    lock.expires_at > now() && pid_alive(lock.pid)
}

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        return std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(windows)]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        return out
            .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
            .unwrap_or(false);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

fn ttl_value(ttl: u64) -> u64 {
    ttl.clamp(1, MAX_TTL)
}

pub fn default_owner() -> String {
    if let Ok(owner) = std::env::var("OMNIX_OWNER") {
        let owner = owner.trim();
        if !owner.is_empty() {
            return owner.chars().take(120).collect();
        }
    }
    if let Ok(pane) = std::env::var("OMNIX_PANE") {
        let pane = pane.trim();
        if !pane.is_empty() {
            return format!("pane:{pane}");
        }
    }
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "owner".into());
    format!("{user}:{}", std::process::id())
}

fn describe_holder(lock: &Lock) -> String {
    let until = lock.expires_at;
    format!(
        "{} (pid {}, expires {})",
        lock.owner, lock.pid, crate::control::local_time(until)
    )
}

/// The PID a lease is tied to. A CLI command is short-lived, so binding the
/// lock to its own PID would make it stale the moment the command returns.
/// The parent is normally the pane shell or the agent process, which stays
/// alive for the work the lock protects; the TTL still bounds abandoned work.
pub fn holder_pid() -> u32 {
    #[cfg(unix)]
    {
        let parent = std::os::unix::process::parent_id();
        if parent > 1 {
            return parent;
        }
    }
    std::process::id()
}

/// Acquire one resource for this caller. Re-acquiring your own live lock
/// refreshes its TTL instead of failing, so a worker can renew.
pub fn acquire_with(
    root: &Path,
    resource: &str,
    owner: &str,
    ttl_secs: u64,
    meta: BTreeMap<String, String>,
) -> Result<Lock> {
    acquire_as(root, resource, owner, ttl_secs, holder_pid(), meta)
}

/// Collect several leases for one operation. On success the caller disarms the
/// guard and stores the ids; on any error the guard releases what it took so a
/// half-created handoff never leaves resources blocked.
pub struct Guard {
    root: PathBuf,
    ids: Vec<String>,
}

impl Guard {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            ids: Vec::new(),
        }
    }

    pub fn acquire(
        &mut self,
        resource: &str,
        owner: &str,
        ttl_secs: u64,
        meta: BTreeMap<String, String>,
    ) -> Result<Lock> {
        let lock = acquire_with(&self.root, resource, owner, ttl_secs, meta)?;
        if !self.ids.contains(&lock.id) {
            self.ids.push(lock.id.clone());
        }
        Ok(lock)
    }

    pub fn ids(&self) -> Vec<String> {
        self.ids.clone()
    }

    pub fn disarm(mut self) -> Vec<String> {
        std::mem::take(&mut self.ids)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if !self.ids.is_empty() {
            let _ = release_ids(&self.root, &self.ids, true);
        }
    }
}

/// Testable entry point: the caller supplies the PID recorded in the lease.
pub fn acquire_as(
    root: &Path,
    resource: &str,
    owner: &str,
    ttl_secs: u64,
    pid: u32,
    meta: BTreeMap<String, String>,
) -> Result<Lock> {
    let resource = normalize(resource)?;
    let owner = owner.trim();
    if owner.is_empty() {
        bail!("lock owner is required");
    }
    let owner: String = owner.chars().take(120).collect();
    let path = path_for(root, &resource);
    let ttl = ttl_value(ttl_secs);

    for _ in 0..3 {
        let stamp = now();
        let lock = Lock {
            id: id_for(&resource),
            resource: resource.clone(),
            owner: owner.clone(),
            pid,
            created_at: stamp,
            expires_at: stamp.saturating_add(ttl),
            meta: meta.clone(),
        };
        if write_new(&path, &lock)? {
            return Ok(lock);
        }
        let Some(existing) = read(&path) else {
            // A corrupt or vanishing file should not block the resource.
            let _ = fs::remove_file(&path);
            continue;
        };
        if existing.owner == owner {
            // Same owner: renew, keep the original creation time, merge meta.
            let mut renewed = existing.clone();
            renewed.pid = pid;
            renewed.expires_at = stamp.saturating_add(ttl);
            renewed.meta.extend(meta.clone());
            write(&path, &renewed)?;
            return Ok(renewed);
        }
        if active(&existing) {
            bail!(
                "resource '{resource}' is locked by {}; wait for it to expire or release it",
                describe_holder(&existing)
            );
        }
        let _ = fs::remove_file(&path);
    }
    bail!("could not acquire '{resource}': lock file kept changing")
}

pub fn get(root: &Path, resource: &str) -> Option<Lock> {
    let resource = normalize(resource).ok()?;
    read(&path_for(root, &resource)).filter(active)
}

/// List live leases and delete the expired ones as a side effect. Listing is
/// the only cleanup path besides re-acquiring the same resource.
pub fn list(root: &Path) -> Vec<Lock> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(locks_dir(root)) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        match read(&path) {
            Some(lock) if active(&lock) => out.push(lock),
            Some(_) => {
                let _ = fs::remove_file(&path);
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    out.sort_by(|a, b| a.resource.cmp(&b.resource));
    out
}

pub fn prune(root: &Path) -> usize {
    let before = fs::read_dir(locks_dir(root))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    let live = list(root).len();
    before.saturating_sub(live)
}

/// Release by resource or lock id. Ownership is enforced unless `force` is set;
/// the handoff close path passes `force` because the record itself carries the
/// ids it created.
pub fn release(root: &Path, resource_or_id: &str, owner: Option<&str>, force: bool) -> Result<Option<Lock>> {
    let wanted = resource_or_id.trim();
    if wanted.is_empty() {
        bail!("lock resource or id is required");
    }
    let mut found = None;
    let Ok(entries) = fs::read_dir(locks_dir(root)) else {
        return Ok(None);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(lock) = read(&path) else {
            let _ = fs::remove_file(&path);
            continue;
        };
        if lock.resource == wanted || lock.id == wanted {
            found = Some((path, lock));
            break;
        }
    }
    let Some((path, lock)) = found else {
        return Ok(None);
    };
    if !force {
        if let Some(owner) = owner {
            if lock.owner != owner.trim() {
                bail!(
                    "resource '{}' is held by {}; pass --force to release it",
                    lock.resource,
                    describe_holder(&lock)
                );
            }
        }
    }
    fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    Ok(Some(lock))
}

pub fn release_ids(root: &Path, ids: &[String], force: bool) -> Vec<String> {
    let mut released = Vec::new();
    for id in ids {
        if let Ok(Some(lock)) = release(root, id, None, force) {
            released.push(lock.resource);
        }
    }
    released
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-lock-{tag}-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn second_owner_is_refused_and_release_reopens_it() {
        let root = temp_root("exclusive");
        let me = std::process::id();
        let first = acquire_as(&root, "port:3000", "agent:a", 60, me, BTreeMap::new()).unwrap();
        assert_eq!(first.resource, "port:3000");
        let err = acquire_as(&root, "port:3000", "agent:b", 60, me, BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("agent:a"), "{err}");
        assert!(release(&root, "port:3000", Some("agent:b"), false).is_err());
        assert!(release(&root, "port:3000", Some("agent:a"), false).unwrap().is_some());
        assert!(acquire_as(&root, "port:3000", "agent:b", 60, me, BTreeMap::new()).is_ok());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn expired_or_dead_leases_are_stale() {
        let root = temp_root("stale");
        let me = std::process::id();
        let lock = acquire_as(&root, "path:src", "agent:a", 60, me, BTreeMap::new()).unwrap();
        let mut stale = lock.clone();
        stale.expires_at = 1;
        write(&path_for(&root, "path:src"), &stale).unwrap();
        assert!(list(&root).is_empty(), "expired lease is pruned");
        assert!(acquire_as(&root, "path:src", "agent:b", 60, me, BTreeMap::new()).is_ok());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn same_owner_renews_without_losing_meta() {
        let root = temp_root("renew");
        let me = std::process::id();
        let mut meta = BTreeMap::new();
        meta.insert("handoff".into(), "task".into());
        let first = acquire_as(&root, "worktree:/tmp/x", "agent:a", 10, me, meta.clone()).unwrap();
        let second = acquire_as(&root, "worktree:/tmp/x", "agent:a", 120, me, BTreeMap::new()).unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.meta.get("handoff").map(String::as_str), Some("task"));
        assert!(second.expires_at >= first.expires_at);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn release_by_id_and_prune() {
        let root = temp_root("ids");
        let me = std::process::id();
        let lock = acquire_as(&root, "branch:main", "agent:a", 60, me, BTreeMap::new()).unwrap();
        assert_eq!(list(&root).len(), 1);
        assert!(release(&root, &lock.id, None, true).unwrap().is_some());
        assert!(list(&root).is_empty());
        let gone = acquire_as(&root, "branch:main", "agent:a", 60, 999_999, BTreeMap::new()).unwrap();
        assert_eq!(prune(&root), 1, "dead PID is pruned");
        assert!(release(&root, &gone.id, None, true).unwrap().is_none());
        fs::remove_dir_all(&root).ok();
    }
}
