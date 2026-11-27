//! Claude Code folder trust, ported from Orca's
//! `src/main/claude/configuration/claude-folder-trust-file.ts`.
//!
//! Claude shows "Is this a project you trust?" for every new folder and exits
//! when nothing answers, which killed Omnix panes. Omnix marks only its own
//! `.omnix/worktrees/` checkouts as trusted.
//!
//! Safety rules copied from Orca:
//! - never create a missing config; never write an unreadable one;
//! - grant nothing when any key is already trusted;
//! - replace the file through a same-mode temporary file and a rename, so a
//!   symlinked config keeps its link and an owner-only mode stays owner-only;
//! - hold Claude Code's own `<file>.lock` directory while reading and writing.
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustOutcome {
    Granted,
    Unchanged,
    Missing,
    Unreadable,
    Locked,
}

impl TrustOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustOutcome::Granted => "granted",
            TrustOutcome::Unchanged => "unchanged",
            TrustOutcome::Missing => "missing-config",
            TrustOutcome::Unreadable => "unreadable",
            TrustOutcome::Locked => "locked",
        }
    }
}

pub fn wants_trust(checkout: &Path) -> bool {
    checkout.components().any(|c| c.as_os_str() == ".omnix")
}

/// Legacy `.config.json` first, then `.claude.json` under `CLAUDE_CONFIG_DIR`
/// or home, with the `-custom-oauth` suffix when that URL is set.
pub fn config_file_at(home: &Path, get: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let config_dir = get("CLAUDE_CONFIG_DIR").filter(|s| !s.is_empty());
    let legacy_dir = match &config_dir {
        Some(dir) => PathBuf::from(nfc(dir)),
        None => home.join(".claude"),
    };
    let legacy_file = legacy_dir.join(".config.json");
    if legacy_file.is_file() {
        return legacy_file;
    }
    let suffix = if get("CLAUDE_CODE_CUSTOM_OAUTH_URL").is_some_and(|s| !s.is_empty()) {
        "-custom-oauth"
    } else {
        ""
    };
    let base = config_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| home.to_path_buf());
    base.join(format!(".claude{suffix}.json"))
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// Lexical absolute form: resolves `.`/`..` without touching the filesystem.
fn lexical(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|dir| dir.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The spelling as given plus the symlink-resolved spelling, NFC normalised.
pub fn trust_keys(worktree: &Path) -> Vec<String> {
    let resolved = lexical(worktree);
    let mut forms = vec![resolved.clone()];
    if let Ok(real) = fs::canonicalize(&resolved) {
        if real != resolved {
            forms.push(real);
        }
    }
    let mut keys: Vec<String> = Vec::new();
    for form in forms {
        let key = nfc(&form.to_string_lossy());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

enum Change {
    Refuse,
    Unchanged,
    /// The full replacement object plus per-key previous values for the undo log.
    Changed(Value, Vec<(String, Value)>),
}

fn apply(config: &Value, keys: &[String]) -> Change {
    let Some(object) = config.as_object() else {
        return Change::Refuse;
    };
    let mut projects = match object.get("projects") {
        None => Map::new(),
        Some(value) => match value.as_object() {
            Some(map) => map.clone(),
            None => return Change::Refuse,
        },
    };
    let already = keys.iter().any(|key| {
        projects
            .get(key)
            .and_then(Value::as_object)
            .and_then(|entry| entry.get("hasTrustDialogAccepted"))
            == Some(&Value::Bool(true))
    });
    if already {
        return Change::Unchanged;
    }
    let mut previous = Vec::new();
    for key in keys {
        let entry = projects
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        previous.push((
            key.clone(),
            entry
                .get("hasTrustDialogAccepted")
                .cloned()
                .unwrap_or(Value::Null),
        ));
        let mut entry = entry;
        entry.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
        projects.insert(key.clone(), Value::Object(entry));
    }
    let mut next = object.clone();
    next.insert("projects".into(), Value::Object(projects));
    Change::Changed(Value::Object(next), previous)
}

enum Target {
    Missing,
    Unreadable,
    File(PathBuf),
}

/// Follow a symlink so the rename rewrites the real file and keeps the link.
fn resolve_target(config_file: &Path) -> Target {
    let Ok(meta) = fs::symlink_metadata(config_file) else {
        return Target::Missing;
    };
    let path = if meta.file_type().is_symlink() {
        match fs::canonicalize(config_file) {
            Ok(real) => real,
            Err(_) => return Target::Unreadable,
        }
    } else {
        config_file.to_path_buf()
    };
    match fs::metadata(&path) {
        Ok(meta) if meta.is_file() => Target::File(path),
        _ => Target::Unreadable,
    }
}

fn read_config(path: &Path) -> Option<Value> {
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    value.is_object().then_some(value)
}

enum Probe {
    Missing,
    Unreadable,
    Ready(PathBuf, Value),
}

fn probe(config_file: &Path) -> Probe {
    match resolve_target(config_file) {
        Target::Missing => Probe::Missing,
        Target::Unreadable => Probe::Unreadable,
        Target::File(path) => match read_config(&path) {
            Some(value) => Probe::Ready(path, value),
            None => Probe::Unreadable,
        },
    }
}

fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{n}", std::process::id())
}

struct Replacement {
    path: PathBuf,
}

impl Drop for Replacement {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Create the same-mode temporary file up front, before the lock, as Orca does.
fn create_replacement(target: &Path) -> Result<Replacement> {
    let meta = fs::metadata(target)?;
    let mode = meta.permissions();
    let mut name = target.as_os_str().to_os_string();
    name.push(format!(".omnix-trust-{}.tmp", unique_suffix()));
    let path = PathBuf::from(name);
    let mut open = OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        open.mode(mode.mode() & 0o777);
        open.open(&path)
            .with_context(|| format!("create {}", path.display()))?;
        // Restore bits the umask narrowed while creating the file.
        fs::set_permissions(&path, fs::Permissions::from_mode(mode.mode() & 0o777))?;
    }
    #[cfg(not(unix))]
    open.open(&path)
        .with_context(|| format!("create {}", path.display()))?;
    Ok(Replacement { path })
}

struct Lock {
    dir: PathBuf,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.dir);
    }
}

/// Claude Code's own lock: an atomically created `<file>.lock` directory.
fn acquire_lock(config_file: &Path) -> Option<Lock> {
    let mut name = config_file.as_os_str().to_os_string();
    name.push(".lock");
    let dir = PathBuf::from(name);
    let delays = [50u64, 100, 200, 250];
    for (attempt, delay) in delays.iter().enumerate() {
        match fs::create_dir(&dir) {
            Ok(()) => return Some(Lock { dir }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if attempt + 1 < delays.len() {
                    std::thread::sleep(Duration::from_millis(*delay));
                }
            }
            Err(_) => return None,
        }
    }
    None
}

/// Omnix writers of one config file queue behind each other.
static TRUST_QUEUE: Mutex<()> = Mutex::new(());

fn write_undo_log(log: &Path, config_file: &Path, entries: &[(String, Value)]) {
    let mut doc: Value = fs::read(log)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({ "version": 1, "created": {} }));
    if !doc["created"].is_object() {
        doc["created"] = serde_json::json!({});
    }
    for (key, previous) in entries {
        if doc["created"].get(key).is_none() {
            doc["created"][key] = serde_json::json!({
                "file": config_file.to_string_lossy(),
                "previous": previous,
            });
        }
    }
    if let Some(parent) = log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = log.with_extension("json.tmp");
    if fs::write(&tmp, serde_json::to_vec_pretty(&doc).unwrap_or_default()).is_ok() {
        let _ = fs::rename(&tmp, log);
    }
}

/// Grant trust for `keys`. Never errors: every failure is an outcome so the
/// pane still launches and Claude shows its own dialog.
pub fn grant(config_file: &Path, keys: &[String], undo_log: Option<&Path>) -> TrustOutcome {
    let _queue = TRUST_QUEUE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match grant_now(config_file, keys, undo_log) {
        Ok(outcome) => outcome,
        Err(_) => TrustOutcome::Unreadable,
    }
}

fn grant_now(config_file: &Path, keys: &[String], undo_log: Option<&Path>) -> Result<TrustOutcome> {
    let (target, config) = match probe(config_file) {
        Probe::Missing => return Ok(TrustOutcome::Missing),
        Probe::Unreadable => return Ok(TrustOutcome::Unreadable),
        Probe::Ready(path, value) => (path, value),
    };
    match apply(&config, keys) {
        Change::Refuse => return Ok(TrustOutcome::Unreadable),
        Change::Unchanged => return Ok(TrustOutcome::Unchanged),
        Change::Changed(..) => {}
    }

    let replacement = create_replacement(&target)?;
    let Some(lock) = acquire_lock(config_file) else {
        return Ok(TrustOutcome::Locked);
    };

    // Re-read under the lock so a concurrent writer's update is not lost.
    let (current_target, current) = match probe(config_file) {
        Probe::Missing => return Ok(TrustOutcome::Missing),
        Probe::Unreadable => return Ok(TrustOutcome::Unreadable),
        Probe::Ready(path, value) => (path, value),
    };
    // A retargeted symlink invalidates the permissions copied above.
    if current_target != target {
        return Ok(TrustOutcome::Unreadable);
    }
    let (next, previous) = match apply(&current, keys) {
        Change::Refuse => return Ok(TrustOutcome::Unreadable),
        Change::Unchanged => return Ok(TrustOutcome::Unchanged),
        Change::Changed(value, previous) => (value, previous),
    };

    let mut bytes = serde_json::to_vec_pretty(&next)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&replacement.path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&replacement.path, &current_target)?;
    drop(lock);

    if let Some(log) = undo_log {
        write_undo_log(log, config_file, &previous);
    }
    Ok(TrustOutcome::Granted)
}

/// Resolve the pane's Claude config file from its effective environment and
/// grant trust for an Omnix worktree. Never fails the launch.
pub fn grant_for_pane(
    profile_env: &[(String, String)],
    extra_env: &[(String, String)],
    worktree: &Path,
) {
    if !wants_trust(worktree) {
        return;
    }
    let get = |key: &str| {
        extra_env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .or_else(|| {
                profile_env
                    .iter()
                    .rev()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.clone())
            })
            .or_else(|| std::env::var(key).ok())
    };
    let home = crate::control::home();
    let config_file = config_file_at(&home, &get);
    let keys = trust_keys(worktree);
    let undo_log = home.join(".omnix").join("trust-created.json");
    let outcome = grant(&config_file, &keys, Some(&undo_log));
    crate::control::event(
        "claude-trust",
        serde_json::json!({
            "worktree": worktree.to_string_lossy(),
            "config": config_file.to_string_lossy(),
            "keys": keys,
            "outcome": outcome.as_str(),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "omnix-trust-{tag}-{}-{nanos}",
                std::process::id(),
                nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_config(file: &Path, value: &Value) {
        fs::write(file, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn read_json(file: &Path) -> Value {
        serde_json::from_slice(&fs::read(file).unwrap()).unwrap()
    }

    #[cfg(unix)]
    fn mode(file: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(file).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn merges_the_key_and_keeps_every_other_field() {
        let dir = TempDir::new("merge");
        let file = dir.path().join(".claude.json");
        write_config(
            &file,
            &json!({
                "oauthAccount": { "emailAddress": "x" },
                "mcpServers": { "a": {} },
                "projects": { "/elsewhere": { "hasTrustDialogAccepted": true, "allowedTools": [] } }
            }),
        );
        let outcome = grant(&file, &["/wt".to_string()], None);
        assert_eq!(outcome, TrustOutcome::Granted);
        assert_eq!(
            read_json(&file),
            json!({
                "oauthAccount": { "emailAddress": "x" },
                "mcpServers": { "a": {} },
                "projects": {
                    "/elsewhere": { "hasTrustDialogAccepted": true, "allowedTools": [] },
                    "/wt": { "hasTrustDialogAccepted": true }
                }
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn keeps_an_owner_only_mode_through_the_rewrite() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("mode");
        let file = dir.path().join(".claude.json");
        write_config(&file, &json!({}));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Granted
        );
        assert_eq!(mode(&file), 0o600, "mode must survive the rename");
    }

    #[test]
    fn leaves_a_corrupt_file_byte_for_byte_untouched() {
        let dir = TempDir::new("corrupt");
        let file = dir.path().join(".claude.json");
        fs::write(&file, "{\"oauthAccount\": ").unwrap();
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Unreadable
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "{\"oauthAccount\": ");
    }

    #[test]
    fn refuses_when_projects_is_not_an_object() {
        let dir = TempDir::new("projects");
        let file = dir.path().join(".claude.json");
        write_config(&file, &json!({ "projects": [] }));
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Unreadable
        );
        assert_eq!(read_json(&file), json!({ "projects": [] }));
    }

    #[test]
    fn a_missing_config_is_not_created() {
        let dir = TempDir::new("missing");
        let file = dir.path().join(".claude.json");
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Missing
        );
        assert!(!file.exists(), "Omnix must not create a missing config");
    }

    #[test]
    fn an_already_trusted_key_writes_nothing() {
        let dir = TempDir::new("unchanged");
        let file = dir.path().join(".claude.json");
        write_config(
            &file,
            &json!({ "projects": { "/wt": { "hasTrustDialogAccepted": true } } }),
        );
        let before = fs::read(&file).unwrap();
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Unchanged
        );
        assert_eq!(fs::read(&file).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_config_stays_a_symlink() {
        use std::os::unix::fs::symlink;
        let dir = TempDir::new("symlink");
        let target = dir.path().join("dotfiles-claude.json");
        let link = dir.path().join(".claude.json");
        write_config(&target, &json!({ "theme": "dark" }));
        symlink(&target, &link).unwrap();
        assert_eq!(
            grant(&link, &["/wt".to_string()], None),
            TrustOutcome::Granted
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link must survive"
        );
        assert_eq!(
            read_json(&target),
            json!({ "theme": "dark", "projects": { "/wt": { "hasTrustDialogAccepted": true } } })
        );
    }

    #[test]
    fn a_lock_held_by_another_writer_blocks_the_write() {
        let dir = TempDir::new("locked");
        let file = dir.path().join(".claude.json");
        write_config(&file, &json!({}));
        let mut lock = file.as_os_str().to_os_string();
        lock.push(".lock");
        fs::create_dir(PathBuf::from(&lock)).unwrap();
        assert_eq!(
            grant(&file, &["/wt".to_string()], None),
            TrustOutcome::Locked
        );
        assert_eq!(read_json(&file), json!({}), "a held lock means no write");
        fs::remove_dir(&lock).unwrap();
    }

    #[test]
    fn concurrent_grants_keep_every_update() {
        let dir = TempDir::new("concurrent");
        let file = dir.path().join(".claude.json");
        write_config(&file, &json!({ "numStartups": 3 }));
        let threads = 8;
        std::thread::scope(|scope| {
            for i in 0..threads {
                let file = file.clone();
                scope.spawn(move || {
                    assert_eq!(
                        grant(&file, &[format!("/wt-{i}")], None),
                        TrustOutcome::Granted
                    );
                });
            }
        });
        let doc = read_json(&file);
        assert_eq!(doc["numStartups"], 3);
        for i in 0..threads {
            assert_eq!(
                doc["projects"][format!("/wt-{i}")]["hasTrustDialogAccepted"],
                true,
                "writer {i} lost its update"
            );
        }
    }

    #[test]
    fn honours_claude_config_dir_and_the_legacy_file() {
        let dir = TempDir::new("configdir");
        let home = dir.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let profile = dir.path().join("profile");
        fs::create_dir_all(&profile).unwrap();
        let get = |key: &str| match key {
            "CLAUDE_CONFIG_DIR" => Some(profile.to_string_lossy().into_owned()),
            _ => None,
        };
        assert_eq!(config_file_at(&home, &get), profile.join(".claude.json"));

        let legacy = profile.join(".config.json");
        write_config(&legacy, &json!({}));
        assert_eq!(config_file_at(&home, &get), legacy);

        fs::remove_file(&legacy).unwrap();
        let custom = |key: &str| match key {
            "CLAUDE_CONFIG_DIR" => Some(profile.to_string_lossy().into_owned()),
            "CLAUDE_CODE_CUSTOM_OAUTH_URL" => Some("https://x".to_string()),
            _ => None,
        };
        assert_eq!(
            config_file_at(&home, &custom),
            profile.join(".claude-custom-oauth.json")
        );
    }

    #[test]
    fn trust_keys_are_nfc_normalised() {
        assert_eq!(nfc("e\u{301}"), "\u{e9}");
    }

    #[test]
    fn only_omnix_checkouts_ask_for_trust() {
        assert!(wants_trust(Path::new("/repo/.omnix/worktrees/a")));
        assert!(!wants_trust(Path::new("/repo/src")));
    }
}
