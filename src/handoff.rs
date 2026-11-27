use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub mode: String,
    pub preferred: String,
    pub reason: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub worktree: String,
    pub branch: String,
    pub base: String,
    pub agent: String,
    pub brief: String,
    pub prompt_sha256: String,
    pub created_at: u64,
    /// journaled | handed-to-terminal | not-delivered | unconfirmed | queued
    pub outcome: String,
    pub receipt: Receipt,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    /// Symlinks planted in the worktree, as `name=source`.
    #[serde(default)]
    pub links: Vec<(String, String)>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub report: Option<String>,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub closed_at: Option<u64>,
    #[serde(default)]
    pub pane_key: Option<String>,
    #[serde(default)]
    pub queued_reason: Option<String>,
    /// Lease ids held by this handoff until `close` releases them.
    #[serde(default)]
    pub locks: Vec<String>,
    /// Owner-configured replacement when the primary agent runs out of quota.
    #[serde(default)]
    pub fallback: Option<crate::fallback::Fallback>,
    /// Quota handling policy (`stop`, `wait`, `fallback=...`).
    #[serde(default)]
    pub on_quota: Option<String>,
    /// Push WIP commit to remote when quota is exhausted mid-run.
    #[serde(default)]
    pub push_wip: bool,
    /// Commit checkpoint on normal clean finish as well.
    #[serde(default)]
    pub checkpoint: bool,
    /// Headless runner PID, log file, session ID, and exit status.
    #[serde(default)]
    pub pid: Option<u32>,
    /// PID of the detached `_headless-worker`, so `close` can stop a queued run.
    #[serde(default)]
    pub worker_pid: Option<u32>,
    #[serde(default)]
    pub log: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub wip_commit: Option<String>,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    /// Queue conditions (`--after`, `--at`, `--min-free-mb`).
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub start_at: Option<u64>,
    #[serde(default)]
    pub min_free_mb: Option<u64>,
    /// True when this handoff is the team orchestrator, so the GUI can mark
    /// its tab and the quota watcher can ignore its chat text.
    #[serde(default)]
    pub orchestrator: bool,
}

/// Everything a CLI or the GUI can decide about a handoff. Defaults keep the
/// old `handoff` command working unchanged.
#[derive(Debug, Default, Clone)]
pub struct CreateOptions {
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub prompt_file: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub base: Option<String>,
    pub worktree_dir: Option<PathBuf>,
    /// Explicit `--branch`; wins over the repo prefix and the role default.
    pub branch: Option<String>,
    pub links: Vec<(String, PathBuf)>,
    pub role: Option<String>,
    pub parent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: Option<String>,
    pub paths: Vec<String>,
    pub fallback: Option<crate::fallback::Fallback>,
    pub on_quota: Option<String>,
    pub push_wip: bool,
    pub checkpoint: bool,
    pub after: Option<String>,
    pub start_at: Option<u64>,
    pub min_free_mb: Option<u64>,
    pub orchestrator: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("spawn git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn slug(name: &str) -> Result<String> {
    let s = name.trim().to_ascii_lowercase();
    if s.is_empty()
        || s.len() > 48
        || !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || s.starts_with('-')
        || s.ends_with('-')
    {
        bail!("name must be 1-48 chars of a-z, 0-9, '-'");
    }
    Ok(s)
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with(".lock")
        && !value.contains("..")
        && !value.contains("//")
        && !value.contains("@{")
        && !value.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
}

/// Branch prefix from `.omnix/config` (`branch_prefix`) else derived from the
/// role name. Plain `omnix/` stays the fallback.
fn branch_prefix(repo: &Path, role: Option<&str>) -> String {
    let configured = fs::read_to_string(repo.join(".omnix").join("config"))
        .ok()
        .and_then(|text| {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                return value
                    .get("branch_prefix")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            text.lines().find_map(|line| {
                let line = line.trim();
                if line.starts_with('#') {
                    return None;
                }
                let (key, value) = line.split_once('=')?;
                (key.trim() == "branch_prefix")
                    .then(|| value.trim().trim_matches('"').to_string())
            })
        })
        .filter(|p| !p.trim().is_empty());
    let prefix = configured.unwrap_or_else(|| {
        let role = role.unwrap_or("").to_ascii_lowercase();
        if role.contains("perbaikan") || role.contains("fix") || role.contains("debug") {
            "fix/".into()
        } else if role.contains("fitur") || role.contains("feature") {
            "fitur/".into()
        } else if role.contains("desain") || role.contains("design") {
            "desain/".into()
        } else if role.contains("server") || role.contains("infra") {
            "server/".into()
        } else {
            "omnix/".into()
        }
    });
    let prefix = prefix.trim().trim_start_matches('/').to_string();
    if prefix.is_empty() {
        "omnix/".into()
    } else if prefix.ends_with('/') {
        prefix
    } else {
        format!("{prefix}/")
    }
}

fn read_capped(path: &Path, limit: usize, what: &str) -> Result<String> {
    let meta = fs::metadata(path).with_context(|| format!("read {what} {}", path.display()))?;
    if meta.len() as usize > limit {
        bail!("{what} is too large (limit {} KiB)", limit / 1024);
    }
    fs::read_to_string(path).with_context(|| format!("read {what} {}", path.display()))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn record_path(record: &Handoff) -> PathBuf {
    PathBuf::from(&record.repo)
        .join(".omnix")
        .join("handoffs")
        .join(format!("{}.json", record.id))
}

fn write_record(record: &Handoff) -> Result<PathBuf> {
    let path = record_path(record);
    atomic_write(&path, &serde_json::to_vec_pretty(record)?)?;
    Ok(path)
}

pub fn save(record: &Handoff) -> Result<PathBuf> {
    write_record(record)
}

/// Global index so `handoff list|report|close` work from any folder.
pub fn index_path() -> PathBuf {
    crate::control::home().join(".omnix").join("handoffs.json")
}

/// Drops a record path from the global index. The record file is left alone.
pub fn forget(path: &Path) -> Result<()> {
    let text: Vec<String> = fs::read(index_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| Path::new(p) != path)
        .collect();
    atomic_write(&index_path(), &serde_json::to_vec_pretty(&text)?)
}

pub fn remember(path: &Path) -> Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut paths: Vec<PathBuf> = fs::read(index_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    if !paths.iter().any(|p| p == &path) {
        paths.push(path);
    }
    let text: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    atomic_write(&index_path(), &serde_json::to_vec_pretty(&text)?)
}

pub fn list_from(paths: &[PathBuf]) -> Vec<Handoff> {
    let mut out = Vec::new();
    for path in paths {
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(record) = serde_json::from_slice::<Handoff>(&bytes) else {
            continue;
        };
        out.push(record);
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// Index entries whose record file no longer exists, typically because the repo
/// was deleted outside Omnix. `list_all` skips them, so `gc` needs this list.
pub fn missing_index_entries() -> Vec<PathBuf> {
    fs::read(index_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| !p.exists())
        .collect()
}

pub fn list_all() -> Vec<Handoff> {
    let paths: Vec<PathBuf> = fs::read(index_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    list_from(&paths)
}

fn records_in(repo: &Path) -> Vec<Handoff> {
    let dir = repo.join(".omnix").join("handoffs");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    list_from(&paths)
}

pub fn find_in(repo: &Path, name: &str) -> Option<Handoff> {
    records_in(repo)
        .into_iter()
        .filter(|record| record.name == name && !record.closed)
        .max_by_key(|record| record.created_at)
}

pub fn find_any(name: &str) -> Option<Handoff> {
    list_all()
        .into_iter()
        .filter(|record| record.name == name && !record.closed)
        .max_by_key(|record| record.created_at)
}

/// Inside a handoff worktree, the main repository is the first entry of
/// `git worktree list`; reports must land beside the other handoffs.
pub fn main_repo(path: &Path) -> Result<PathBuf> {
    let top = git(path, &["rev-parse", "--show-toplevel"])?;
    let text = git(Path::new(&top), &["worktree", "list", "--porcelain"])?;
    let first = text
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
        .unwrap_or(&top);
    Ok(PathBuf::from(first))
}

#[derive(Debug, Clone)]
pub struct PaneWorktree {
    pub name: String,
    pub worktree: PathBuf,
    pub branch: String,
}

/// Create a fresh worktree for a live pane that shares a folder with another
/// agent. The pane's agent session is resumed by the GUI in the new folder; no
/// handoff brief is written because the worker's context already exists.
pub fn worktree_for_pane(repo: &Path, pane_key: &str, label: &str) -> Result<PaneWorktree> {
    let repo = main_repo(repo)?;
    let base = git(&repo, &["rev-parse", "--short", "HEAD"])?;
    let label: String = {
        let mut out = String::new();
        for ch in label.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if !out.ends_with('-') {
                out.push('-');
            }
            if out.len() >= 24 {
                break;
            }
        }
        out.trim_matches('-').to_string()
    };
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{pane_key}:{}", now()).as_bytes())
    );
    let stem = if label.is_empty() {
        format!("pindah-{}", &digest[..8])
    } else {
        format!("pindah-{label}-{}", &digest[..6])
    };
    let name = slug(&stem)?;
    let branch = format!("omnix/{name}");
    let worktree = repo.join(".omnix").join("worktrees").join(&name);
    if worktree.exists() {
        bail!("worktree already exists: {}", worktree.display());
    }
    fs::create_dir_all(worktree.parent().context("worktree has no parent")?)?;

    let path = worktree.to_string_lossy().to_string();
    let add = ["worktree", "add", "-b", branch.as_str(), &path, &base];
    if let Err(first) = git(&repo, &add) {
        git(&repo, &["worktree", "add", &path, &branch])
            .with_context(|| format!("create move worktree failed first as: {first}"))?;
    }
    crate::control::event(
        "pane-moved-worktree",
        serde_json::json!({
            "pane": pane_key,
            "name": name,
            "worktree": path,
            "branch": branch,
        }),
    );
    Ok(PaneWorktree {
        name,
        worktree,
        branch,
    })
}

fn resolve_role(role: &str) -> Result<String> {
    let path = PathBuf::from(role);
    if path.is_file() {
        return read_capped(&path, 256 * 1024, "role card");
    }
    let home = crate::control::home();
    let candidate = home
        .join(".claude")
        .join("skills")
        .join(role)
        .join("SKILL.md");
    if candidate.is_file() {
        return read_capped(&candidate, 256 * 1024, "role card");
    }
    bail!(
        "role '{role}' not found (looked for {} and ~/.claude/skills/{role}/SKILL.md)",
        path.display()
    )
}

fn validate_paths(paths: &[String]) -> Result<()> {
    for pattern in paths {
        if pattern.is_empty()
            || pattern.len() > 256
            || pattern.starts_with('-')
            || pattern.chars().any(char::is_control)
        {
            bail!("invalid --paths pattern '{pattern}'");
        }
    }
    Ok(())
}

fn brief_body(
    name: &str,
    agent: &str,
    repo: &Path,
    branch: &str,
    base: &str,
    id: &str,
    parent: Option<&str>,
    mode: &str,
    paths: &[String],
    role: Option<&str>,
    prompt: &str,
) -> String {
    let mut body = format!(
        "# Handoff: {name}\n\n- Agent: {agent}\n- Repo: {}\n- Branch: {branch}\n- Base: {base}\n- Mode: {mode}\n- Created: {} ({id})\n",
        repo.display(),
        crate::control::local_time(now()),
    );
    if let Some(parent) = parent {
        body.push_str(&format!("- Parent: {parent}\n"));
    }
    if !paths.is_empty() {
        body.push_str(&format!("- Paths: {}\n", paths.join(", ")));
    }
    if let Some(role) = role {
        body.push_str("\n## Role card\n\n");
        body.push_str(role.trim());
        body.push('\n');
    }
    body.push_str("\n## Task\n\n");
    body.push_str(prompt.trim());
    body.push_str(
        "\n\n## Report back\n\n\
When the work is done or you are blocked, write a short report and run:\n\n\
```sh\n",
    );
    body.push_str(&format!(
        "omnix handoff report {name} --status done --file <report.md>\n"
    ));
    body.push_str("```\n\nUse `--status blocked` or `--status failed` when that is the truth.\n");
    body
}

/// Undoes the worktree, branch, brief, and record if `create_with` fails after
/// touching git, so a failed handoff never blocks the next attempt with
/// "already open" or "worktree already exists".
struct CreateRollback {
    repo: PathBuf,
    worktree: PathBuf,
    branch: String,
    brief: Option<PathBuf>,
    record: Option<PathBuf>,
    armed: bool,
}

impl CreateRollback {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CreateRollback {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let path = self.worktree.to_string_lossy().to_string();
        let _ = git(&self.repo, &["worktree", "remove", "--force", &path]);
        let _ = git(&self.repo, &["branch", "-D", &self.branch]);
        if let Some(brief) = &self.brief {
            let _ = fs::remove_file(brief);
        }
        if let Some(record) = &self.record {
            let _ = fs::remove_file(record);
        }
    }
}

/// Mirrors Orca: create worktree, write the brief durably, then deliver. The
/// caller decides delivery (the GUI control channel) so a dry run stays dry.
pub fn create_with(root: &Path, options: CreateOptions) -> Result<(Handoff, PathBuf)> {
    let name = slug(&options.name)?;
    let agent = if options.agent.trim().is_empty() {
        "codex".to_string()
    } else {
        options.agent.clone()
    };
    if crate::agent::launch_cmd(&agent).is_none() {
        bail!(
            "unknown agent '{agent}' (known: {})",
            crate::agent::AGENTS.join(", ")
        );
    }
    let mode = options.mode.clone();
    if let Some(mode) = &mode {
        if !matches!(mode.as_str(), "plan" | "edit" | "full") {
            bail!("--mode must be plan, edit, or full");
        }
    }
    if let Some(fb) = &options.fallback {
        if fb.agent.is_empty() && fb.command.is_none() {
            bail!("--fallback needs an agent or --fallback-command");
        }
        if !fb.agent.is_empty() && !crate::agent::known(&fb.agent) {
            bail!("unknown fallback agent '{}'", fb.agent);
        }
        if let Some(command) = fb.command.as_deref() {
            if command.trim().is_empty() || command.contains('\n') || command.len() > 2000 {
                bail!("--fallback-command must be one non-empty line (max 2000 chars)");
            }
        }
        if fb.agent == agent {
            bail!("fallback agent must differ from the primary agent");
        }
    }
    validate_paths(&options.paths)?;
    if let Some(parent) = &options.parent {
        if !valid_label(parent) {
            bail!("invalid --parent label");
        }
    }

    let prompt = match &options.prompt_file {
        Some(path) => {
            let text = read_capped(path, 1024 * 1024, "prompt file")?;
            if options.prompt.trim().is_empty() {
                text
            } else {
                format!("{}\n\n{}", text.trim_end(), options.prompt.trim())
            }
        }
        None => options.prompt.clone(),
    };
    if prompt.trim().is_empty() {
        bail!("--prompt or --prompt-file is required");
    }

    let repo = match &options.repo {
        Some(path) => {
            let path = if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()?.join(path)
            };
            PathBuf::from(git(&path, &["rev-parse", "--show-toplevel"])?)
        }
        None => PathBuf::from(git(root, &["rev-parse", "--show-toplevel"])?),
    };
    if !repo.is_dir() {
        bail!("repo not found: {}", repo.display());
    }
    if let Some(fb) = &options.fallback {
        crate::fallback::set(&repo, &agent, fb.clone())?;
    }
    if let Some(existing) = find_in(&repo, &name) {
        bail!(
            "handoff '{name}' is already open ({}); close it or pick another name",
            existing.worktree
        );
    }

    let base = match &options.base {
        Some(base) => base.clone(),
        None => match git(&repo, &["rev-parse", "--verify", "--quiet", "origin/main"]) {
            Ok(_) => "origin/main".to_string(),
            Err(_) => git(&repo, &["rev-parse", "--short", "HEAD"])?,
        },
    };
    let branch = match options.branch.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            let raw = raw.trim();
            if !valid_branch(raw) {
                bail!("--branch must be a valid git branch name");
            }
            raw.to_string()
        }
        _ => format!("{}{name}", branch_prefix(&repo, options.role.as_deref())),
    };
    let worktree = match &options.worktree_dir {
        Some(path) => {
            if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()?.join(path)
            }
        }
        None => repo.join(".omnix").join("worktrees").join(&name),
    };
    // Resolve the role and validate link names before touching git, so a bad
    // argument fails without leaving a worktree behind.
    let role = match &options.role {
        Some(role) => Some(resolve_role(role)?),
        None => None,
    };
    for (link_name, _) in &options.links {
        if link_name.is_empty()
            || link_name.len() > 128
            || link_name.starts_with('/')
            || link_name.contains("..")
            || link_name.chars().any(char::is_control)
        {
            bail!("invalid --link name '{link_name}'");
        }
    }
    if worktree.exists() {
        bail!("worktree already exists: {}", worktree.display());
    }

    // Lease the worktree, branch, and declared paths before touching git. The
    // guard releases everything if a later step fails.
    let owner = crate::locks::default_owner();
    let mut guard = crate::locks::Guard::new(&repo);
    let mut meta = BTreeMap::new();
    meta.insert("handoff".into(), name.clone());
    meta.insert("agent".into(), agent.clone());
    guard.acquire(
        &format!("worktree:{}", worktree.display()),
        &owner,
        crate::locks::DEFAULT_TTL,
        meta.clone(),
    )?;
    guard.acquire(
        &format!("branch:{branch}"),
        &owner,
        crate::locks::DEFAULT_TTL,
        meta.clone(),
    )?;
    for pattern in &options.paths {
        let mut path_meta = meta.clone();
        path_meta.insert("pattern".into(), pattern.clone());
        guard.acquire(
            &format!("path:{pattern}"),
            &owner,
            crate::locks::DEFAULT_TTL,
            path_meta,
        )?;
    }

    fs::create_dir_all(worktree.parent().context("worktree has no parent")?)?;

    let mut add = vec!["worktree", "add", "-b", branch.as_str()];
    let path = worktree.to_string_lossy().to_string();
    add.push(&path);
    if base != "HEAD" {
        add.push(&base);
    }
    if let Err(first) = git(&repo, &add) {
        // Branch may already exist; attach to it instead of creating another.
        git(&repo, &["worktree", "add", &path, &branch])
            .with_context(|| format!("create worktree failed first as: {first}"))?;
    }

    let mut rollback = CreateRollback {
        repo: repo.clone(),
        worktree: worktree.clone(),
        branch: branch.clone(),
        brief: None,
        record: None,
        armed: true,
    };

    let mut links = Vec::new();
    for (link_name, source) in &options.links {
        let dest = worktree.join(link_name);
        if dest.exists() || dest.symlink_metadata().is_ok() {
            bail!("{} already exists in the worktree", dest.display());
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        symlink(source, &dest)
            .with_context(|| format!("link {} -> {}", dest.display(), source.display()))?;
        links.push((
            link_name.clone(),
            source.to_string_lossy().into_owned(),
        ));
    }

    let id = format!("{}-{name}", now());
    let handoff_dir = repo.join(".omnix").join("handoffs");
    fs::create_dir_all(&handoff_dir)?;
    let brief = handoff_dir.join(format!("{id}.md"));
    let body = brief_body(
        &name,
        &agent,
        &repo,
        &branch,
        &base,
        &id,
        options.parent.as_deref(),
        mode.as_deref().unwrap_or("default"),
        &options.paths,
        role.as_deref(),
        &prompt,
    );
    atomic_write(&brief, body.as_bytes())?;
    rollback.brief = Some(brief.clone());

    let record = Handoff {
        id: id.clone(),
        name: name.clone(),
        repo: repo.to_string_lossy().to_string(),
        worktree: worktree.to_string_lossy().to_string(),
        branch,
        base,
        agent: agent.clone(),
        brief: brief.to_string_lossy().to_string(),
        prompt_sha256: format!("{:x}", Sha256::digest(prompt.as_bytes())),
        created_at: now(),
        outcome: "not-delivered".into(),
        receipt: Receipt {
            mode: "terminal".into(),
            preferred: "terminal".into(),
            reason: "cli-handoff".into(),
            detail: "dry run: worktree and brief written; prompt not injected".into(),
        },
        warning: None,
        role: options.role,
        parent: options.parent,
        model: options.model,
        effort: options.effort,
        mode,
        paths: options.paths,
        links,
        status: None,
        report: None,
        updated_at: now(),
        closed: false,
        closed_at: None,
        pane_key: None,
        queued_reason: None,
        locks: guard.ids(),
        fallback: options.fallback.clone(),
        on_quota: options.on_quota,
        push_wip: options.push_wip,
        checkpoint: options.checkpoint,
        pid: None,
        worker_pid: None,
        log: None,
        session_id: None,
        exit_code: None,
        wip_commit: None,
        blocked_reason: None,
        after: options.after,
        start_at: options.start_at,
        min_free_mb: options.min_free_mb,
        orchestrator: options.orchestrator,
    };
    let record_path = write_record(&record)?;
    rollback.record = Some(record_path.clone());
    // The record now owns the lease ids; the guard must not release them.
    let _ = guard.disarm();
    rollback.disarm();
    crate::control::event(
        "handoff-created",
        serde_json::json!({
            "name": record.name,
            "agent": record.agent,
            "repo": record.repo,
            "worktree": record.worktree,
            "brief": record.brief,
            "parent": record.parent,
            "mode": record.mode,
            "paths": record.paths,
            "locks": record.locks,
        }),
    );
    Ok((record, record_path))
}

/// Backwards-compatible entry point: old callers get the same artifacts and
/// the same dry-run receipt. Delivery moved to the caller.
#[allow(dead_code)]
pub fn create(
    root: &Path,
    name: &str,
    agent: &str,
    prompt: &str,
    base: Option<&str>,
    _run: bool,
) -> Result<(Handoff, PathBuf)> {
    create_with(
        root,
        CreateOptions {
            name: name.to_string(),
            agent: agent.to_string(),
            prompt: prompt.to_string(),
            base: base.map(str::to_string),
            ..Default::default()
        },
    )
}

pub fn mark_delivered(record: &mut Handoff, pane_key: &str) -> Result<()> {
    record.outcome = "handed-to-terminal".into();
    record.receipt.detail = format!("prompt queued to pane {pane_key}");
    record.pane_key = Some(pane_key.to_string());
    record.updated_at = now();
    write_record(record)?;
    crate::control::event(
        "handoff-delivered",
        serde_json::json!({ "name": record.name, "pane": pane_key }),
    );
    Ok(())
}

pub fn mark_queued(record: &mut Handoff, reason: &str) -> Result<()> {
    record.outcome = "queued".into();
    record.receipt.detail = format!("queued: {reason}");
    record.queued_reason = Some(reason.to_string());
    record.updated_at = now();
    write_record(record)?;
    crate::control::event(
        "handoff-queued",
        serde_json::json!({ "name": record.name, "reason": reason }),
    );
    Ok(())
}

/// Record the result of a non-interactive run. The transcript stays in the
/// log file; the record only points at it.
pub fn mark_running(record: &mut Handoff, pid: u32) -> Result<()> {
    record.outcome = "working".into();
    record.receipt.detail = format!("headless worker running (pid {pid})");
    record.worker_pid = Some(pid);
    record.updated_at = now();
    write_record(record)?;
    Ok(())
}

pub fn mark_headless(record: &mut Handoff, log: &Path, success: bool) -> Result<()> {
    record.outcome = if success {
        "delivered-headless".into()
    } else {
        "failed".into()
    };
    record.receipt.mode = "headless".into();
    record.receipt.detail = format!(
        "headless run {} -> {}",
        if success { "finished" } else { "failed" },
        log.display()
    );
    record.updated_at = now();
    write_record(record)?;
    crate::control::event(
        "handoff-headless",
        serde_json::json!({ "name": record.name, "ok": success, "log": log }),
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct FallbackTarget {
    pub agent: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuotaPolicy {
    Stop,
    Wait,
    Fallback(Vec<FallbackTarget>),
}

pub fn parse_on_quota(root: &Path, spec: Option<&str>) -> Result<QuotaPolicy> {
    let Some(raw) = spec.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(QuotaPolicy::Stop);
    };
    if raw.eq_ignore_ascii_case("stop") {
        return Ok(QuotaPolicy::Stop);
    }
    if raw.eq_ignore_ascii_case("wait") {
        return Ok(QuotaPolicy::Wait);
    }
    if let Some(list) = raw
        .strip_prefix("fallback=")
        .or_else(|| raw.strip_prefix("fallback:"))
    {
        let mut targets = Vec::new();
        for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (agent, model) = crate::usage::split_agent_model(root, item);
            if !crate::agent::known(&agent) {
                bail!("unknown fallback agent '{agent}' in --on-quota");
            }
            targets.push(FallbackTarget {
                agent,
                model: model.filter(|m| !m.is_empty()),
            });
        }
        if targets.is_empty() {
            bail!("--on-quota fallback= needs at least one agent[:model]");
        }
        return Ok(QuotaPolicy::Fallback(targets));
    }
    bail!("--on-quota must be stop, wait, or fallback=<agent>:<model>[,...]");
}

/// Parse `HH:MM` (24-hour local time) into the next matching Unix epoch.
pub fn parse_at_hhmm(spec: &str) -> Result<u64> {
    let (h, m) = spec
        .trim()
        .split_once(':')
        .context("--at must be HH:MM (24-hour local time)")?;
    let hour: i64 = h.parse().context("--at hour must be 00-23")?;
    let min: i64 = m.parse().context("--at minute must be 00-59")?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&min) {
        bail!("--at must be valid HH:MM (00:00..23:59)");
    }
    let now = now();
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let stamp = now as libc::time_t;
        if !unsafe { libc::localtime_r(&stamp, &mut tm) }.is_null() {
            let target = hour * 3600 + min * 60;
            let current = tm.tm_hour as i64 * 3600 + tm.tm_min as i64 * 60 + tm.tm_sec as i64;
            let mut delta = target - current;
            if delta <= 0 {
                delta += 86_400;
            }
            return Ok((now as i64 + delta) as u64);
        }
    }
    Ok(now)
}

pub fn git_dirty_files(worktree: &Path) -> Vec<String> {
    let Ok(out) = git(worktree, &["status", "--short"]) else {
        return Vec::new();
    };
    out.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn commit_wip(worktree: &Path, message: &str, push: bool) -> Result<Option<String>> {
    let dirty = git_dirty_files(worktree);
    if dirty.is_empty() {
        return Ok(None);
    }
    git(worktree, &["add", "-A"])?;
    let has_user = git(worktree, &["config", "user.name"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        && git(worktree, &["config", "user.email"])
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    if has_user {
        git(worktree, &["commit", "-m", message])?;
    } else {
        git(
            worktree,
            &[
                "-c",
                "user.name=Omnix",
                "-c",
                "user.email=omnix@local",
                "commit",
                "-m",
                message,
            ],
        )?;
    }
    let sha = git(worktree, &["rev-parse", "--short", "HEAD"])?;
    if push {
        let _ = git(worktree, &["push", "origin", "HEAD"]);
    }
    Ok(Some(sha))
}

#[derive(Debug, Clone)]
pub struct WipQuotaOutcome {
    pub wip_sha: Option<String>,
}

/// Commit WIP changes, file a blocked report, and record `agent-quota-exhausted`.
pub fn save_wip_and_report(
    record: &mut Handoff,
    agent: &str,
    model: Option<&str>,
    pane: Option<&str>,
    pesan_asli: &str,
    ex: &crate::usage::Exhaustion,
    push_wip: bool,
) -> Result<WipQuotaOutcome> {
    let worktree = PathBuf::from(&record.worktree);
    let dirty_files = git_dirty_files(&worktree);
    let dirty_count = dirty_files.len();
    let stamp = crate::control::local_time(now());
    let wip_msg = format!("WIP [omnix] kuota {agent} habis {stamp}");
    let wip_sha = if worktree.is_dir() && dirty_count > 0 {
        commit_wip(&worktree, &wip_msg, push_wip).ok().flatten()
    } else {
        None
    };

    let pesan_trimmed: String = pesan_asli.trim().chars().take(300).collect();
    let mut body = format!(
        "# Blocked: kuota {agent} habis\n\n- Handoff: {}\n- Agent: {agent}\n",
        record.name
    );
    if let Some(m) = model.filter(|m| !m.trim().is_empty()) {
        body.push_str(&format!("- Model: {m}\n"));
    }
    if let Some(reset_local) = &ex.reset_local {
        body.push_str(&format!("- Pulih: {reset_local}\n"));
    }
    if let Some(sha) = &wip_sha {
        body.push_str(&format!("- Komit WIP: {sha}\n"));
    } else {
        body.push_str("- Komit WIP: tidak ada perubahan belum commit\n");
    }
    body.push_str(&format!("\n## Pesan asli\n\n{pesan_trimmed}\n"));
    if !dirty_files.is_empty() {
        body.push_str("\n## Berkas berubah\n\n");
        for f in &dirty_files {
            body.push_str(&format!("- {f}\n"));
        }
    }

    let repo = PathBuf::from(&record.repo);
    let reported = report(&repo, &record.name, "blocked", &body)?;
    record.status = reported.status;
    record.report = reported.report;
    record.outcome = "blocked: quota".into();
    record.blocked_reason = Some("quota".into());
    if wip_sha.is_some() {
        record.wip_commit = wip_sha.clone();
    }
    record.updated_at = now();
    write_record(record)?;

    let _ = crate::control::append_event(
        &crate::control::events_path(),
        "agent-quota-exhausted",
        serde_json::json!({
            "agent": agent,
            "model": model.filter(|m| !m.trim().is_empty()),
            "handoff": record.name,
            "pane": pane.or(record.pane_key.as_deref()),
            "resetAt": ex.reset_at,
            "resetLocal": ex.reset_local,
            "retryable": ex.retryable,
            "confidence": ex.confidence,
            "pesanAsli": pesan_trimmed,
            "perubahanBelumCommit": dirty_count,
            "wipCommit": wip_sha,
        }),
    );
    Ok(WipQuotaOutcome { wip_sha })
}

pub fn format_launch_preview(root: &Path, record: &Handoff) -> String {
    let launch = crate::agent::LaunchOptions {
        model: record.model.clone(),
        effort: record.effort.clone(),
        mode: (crate::agent::base_agent(&record.agent) == "antigravity")
            .then(|| record.mode.clone())
            .flatten(),
        prompt: None,
    };
    let extra = crate::agent::extra_args(&record.agent, &launch).unwrap_or_default();
    let argv = if crate::control::read_control(&crate::control::runtime_dir()).is_none() {
        crate::agent::headless_args(&record.agent, &launch, "<brief>").unwrap_or(extra)
    } else {
        extra
    };
    let _ = root;
    let bin = crate::agent::launch_cmd(&record.agent).unwrap_or(&record.agent);
    let rendered = argv
        .iter()
        .map(|arg| {
            if arg.contains(' ') || arg.is_empty() {
                format!("{arg:?}")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if rendered.is_empty() {
        format!("cd {} && {bin}", record.worktree)
    } else {
        format!("cd {} && {bin} {rendered}", record.worktree)
    }
}

/// Machine status view for a headless handoff so `agent status` and `agent wait`
/// also work when the GUI app is not running.
pub fn headless_pane_row(record: &Handoff) -> serde_json::Value {
    let state = if record.blocked_reason.as_deref() == Some("quota")
        || record.outcome == "blocked: quota"
    {
        "quota"
    } else if record.outcome == "queued" {
        "queued"
    } else if record.exit_code.is_some()
        || matches!(
            record.outcome.as_str(),
            "delivered-headless" | "failed"
        )
    {
        "exited"
    } else if record.pid.is_some() {
        "working"
    } else {
        "unknown"
    };
    serde_json::json!({
        "paneKey": record.pane_key.clone().unwrap_or_else(|| format!("ho:{}", record.name)),
        "label": record.name,
        "title": record.name,
        "cwd": record.worktree,
        "agent": record.agent,
        "model": record.model,
        "handoff": record.name,
        "parent": record.parent,
        "sessionId": record.session_id.clone().unwrap_or_default(),
        "status": record.status.clone().unwrap_or_else(|| state.to_string()),
        "state": state,
        "pid": record.pid,
        "log": record.log,
        "exitCode": record.exit_code,
        "wipCommit": record.wip_commit,
        "updatedAt": record.updated_at,
        "updatedAtLocal": crate::control::local_time(record.updated_at),
    })
}

pub fn report(repo: &Path, name: &str, status: &str, text: &str) -> Result<Handoff> {
    let name = slug(name)?;
    if !matches!(status, "done" | "blocked" | "failed") {
        bail!("--status must be done, blocked, or failed");
    }
    if text.trim().is_empty() {
        bail!("report is empty");
    }
    let repo = main_repo(repo)?;
    let mut record =
        find_in(&repo, &name).with_context(|| format!("no open handoff '{name}' in {}", repo.display()))?;
    let path = PathBuf::from(&record.repo)
        .join(".omnix")
        .join("handoffs")
        .join(format!("{}.report.md", record.id));
    atomic_write(&path, text.as_bytes())?;
    record.status = Some(status.to_string());
    record.report = Some(path.to_string_lossy().into_owned());
    record.updated_at = now();
    write_record(&record)?;
    crate::control::event(
        "handoff-report",
        serde_json::json!({
            "name": record.name,
            "status": status,
            "report": record.report,
            "pane": record.pane_key,
        }),
    );
    Ok(record)
}

fn symlink(source: &Path, dest: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, dest)?;
    #[cfg(windows)]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, dest)?;
        } else {
            std::os::windows::fs::symlink_file(source, dest)?;
        }
    }
    Ok(())
}

fn remove_link(path: &Path) -> Result<()> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn dirty_except(worktree: &Path, links: &[(String, String)]) -> Result<bool> {
    let patterns: Vec<String> = links
        .iter()
        .map(|(name, _)| format!(":(exclude){name}"))
        .collect();
    let mut args: Vec<&str> = vec!["status", "--porcelain", "--", "."];
    for pattern in &patterns {
        args.push(pattern);
    }
    Ok(!git(worktree, &args)?.is_empty())
}

fn branch_merged(repo: &Path, branch: &str, base: &str) -> bool {
    let Ok(text) = git(repo, &["branch", "--merged", base]) else {
        return false;
    };
    text.lines()
        .map(|line| line.trim().trim_start_matches("* ").trim())
        .any(|line| line == branch)
}

pub fn close(name: &str) -> Result<Handoff> {
    close_forced(name, false)
}

pub fn close_forced(name: &str, force: bool) -> Result<Handoff> {
    let name = slug(name)?;
    let record = find_any(&name).with_context(|| format!("no open handoff '{name}'"))?;
    close_record_with(record, force)
}

/// Names of open records whose repo directory is gone. Their worktree cannot
/// be reached anymore, so the index entry only blocks `gc` and `handoff list`.
pub fn prune_orphans(records: &[Handoff]) -> Vec<String> {
    records
        .iter()
        .filter(|r| !r.closed && !Path::new(&r.repo).is_dir())
        .map(|r| r.name.clone())
        .collect()
}

pub fn close_in(repo: &Path, name: &str) -> Result<Handoff> {
    close_in_forced(repo, name, false)
}

pub fn close_in_forced(repo: &Path, name: &str, force: bool) -> Result<Handoff> {
    let name = slug(name)?;
    let record = find_in(repo, &name)
        .with_context(|| format!("no open handoff '{name}' in {}", repo.display()))?;
    close_record_with(record, force)
}

fn terminate(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
}

fn close_record(record: Handoff) -> Result<Handoff> {
    close_record_with(record, false)
}

/// `force` discards uncommitted work and leftover folders that git no longer
/// tracks as a worktree. Callers must only pass it after the user confirmed it.
fn close_record_with(mut record: Handoff, force: bool) -> Result<Handoff> {
    // Stop a running or still-queued worker before the worktree disappears.
    for pid in [record.worker_pid, record.pid].into_iter().flatten() {
        terminate(pid);
    }
    let worktree = PathBuf::from(&record.worktree);
    if worktree.exists() {
        let repo = PathBuf::from(&record.repo);
        if force {
            // git refuses `worktree remove` once the .git file is gone, so drop
            // the folder directly and let prune clear git's registration.
            fs::remove_dir_all(&worktree)?;
            let _ = git(&repo, &["worktree", "prune"]);
        } else {
            if dirty_except(&worktree, &record.links)? {
                bail!(
                    "worktree has uncommitted changes; commit or discard them first: {}",
                    worktree.display()
                );
            }
            for (link_name, _) in &record.links {
                remove_link(&worktree.join(link_name))?;
            }
            git(&repo, &["worktree", "remove", &record.worktree])?;
        }
    } else {
        let repo = PathBuf::from(&record.repo);
        let _ = git(&repo, &["worktree", "prune"]);
    }
    let repo = PathBuf::from(&record.repo);
    if branch_merged(&repo, &record.branch, &record.base) {
        let _ = git(&repo, &["branch", "-d", &record.branch]);
    }
    // The handoff record is the authority for its own leases, so release by id
    // even when the closing process is not the original owner.
    let released = crate::locks::release_ids(&repo, &record.locks, true);
    record.closed = true;
    record.closed_at = Some(now());
    record.updated_at = now();
    write_record(&record)?;
    crate::control::event(
        "handoff-closed",
        serde_json::json!({
            "name": record.name,
            "worktree": record.worktree,
            "locks": released,
        }),
    );
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-handoff-{tag}-{}-{}",
            std::process::id(),
            now()
        ));
        if std::env::var_os("OMNIX_HOME").is_none() {
            let home = std::env::temp_dir().join(format!("omnix-test-home-{}", std::process::id()));
            fs::create_dir_all(&home).unwrap();
            unsafe { std::env::set_var("OMNIX_HOME", &home) };
        }
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).unwrap();
        fs::write(dir.join("README.md"), "x\n").unwrap();
        git(&dir, &["add", "."]).unwrap();
        let out = Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=T",
                "commit",
                "-qm",
                "init",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        dir
    }

    #[test]
    fn slug_rules() {
        assert_eq!(slug("fix-login").unwrap(), "fix-login");
        assert!(slug("Fix Login").is_err());
        assert!(slug("-bad").is_err());
        assert!(slug("").is_err());
    }

    #[test]
    fn dry_run_creates_worktree_brief_and_record() {
        let dir = temp_repo("dry");
        let (record, path) =
            create(&dir, "task-one", "codex", "do the thing", None, false).unwrap();
        assert_eq!(record.outcome, "not-delivered");
        assert!(Path::new(&record.worktree).exists());
        assert!(Path::new(&record.brief).exists());
        assert!(path.exists());
        assert!(
            fs::read_to_string(&record.brief)
                .unwrap()
                .contains("do the thing")
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_create_rolls_back_worktree_and_record() {
        let dir = temp_repo("rollback");
        let src = dir.join("source.txt");
        fs::write(&src, "s\n").unwrap();
        let err = create_with(
            &dir,
            CreateOptions {
                name: "probe".into(),
                agent: "codex".into(),
                prompt: "do it".into(),
                links: vec![("README.md".into(), src)],
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("already exists in the worktree"), "{err}");
        assert!(
            !dir.join(".omnix/worktrees/probe").exists(),
            "worktree must be rolled back"
        );
        assert!(find_in(&dir, "probe").is_none(), "record must be rolled back");
        let branch = git(&dir, &["branch", "--list", "omnix/probe"]).unwrap();
        assert!(branch.trim().is_empty(), "branch must be rolled back: {branch}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn queued_handoff_row_has_a_state_and_status() {
        let dir = temp_repo("queued-row");
        let (mut record, _) = create(&dir, "q", "codex", "x", None, false).unwrap();
        record.outcome = "queued".into();
        record.status = None;
        let row = headless_pane_row(&record);
        assert_eq!(row["state"], "queued");
        assert_eq!(row["status"], "queued");
        assert!(!row["status"].is_null());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn close_stops_the_recorded_worker() {
        let dir = temp_repo("close-worker");
        let (mut record, _) = create(&dir, "stopme", "codex", "x", None, false).unwrap();
        let mut child = Command::new("sh")
            .args(["-c", "sleep 60"])
            .spawn()
            .unwrap();
        record.worker_pid = Some(child.id());
        save(&record).unwrap();
        close_in(&dir, "stopme").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let exited = child.try_wait().unwrap().is_some();
        if !exited {
            let _ = child.kill();
        }
        assert!(exited, "queued worker must be stopped by close");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_worktree_starts_from_origin_main_not_local_head() {
        let dir = temp_repo("base");
        let pushed = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        git(&dir, &["update-ref", "refs/remotes/origin/main", &pushed]).unwrap();
        fs::write(dir.join("README.md"), "ahead\n").unwrap();
        git(
            &dir,
            &["-c", "user.email=t@example.com", "-c", "user.name=T", "commit", "-qam", "ahead"],
        )
        .unwrap();

        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "base1".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(record.base, "origin/main");
        let wt = git(Path::new(&record.worktree), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(wt, pushed, "worktree must start at origin/main, not local HEAD");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn branch_prefix_follows_role_config_and_explicit_flag() {
        let dir = temp_repo("branch");
        let role = dir.join("perbaikan.md");
        fs::write(&role, "# Role\nfix things\n").unwrap();
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "a".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                role: Some(role.to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(record.branch, "fix/a");

        fs::create_dir_all(dir.join(".omnix")).unwrap();
        fs::write(dir.join(".omnix/config"), "branch_prefix = fitur/\n").unwrap();
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "b".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(record.branch, "fitur/b");

        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "c".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                branch: Some("hotfix/c".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(record.branch, "hotfix/c");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn orchestrator_flag_survives_the_record() {
        let dir = temp_repo("orchestrator");
        let (record, path) = create_with(
            &dir,
            CreateOptions {
                name: "orch".into(),
                agent: "claude".into(),
                prompt: "x".into(),
                orchestrator: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(record.orchestrator);
        let raw = fs::read_to_string(path).unwrap();
        assert!(raw.contains("\"orchestrator\": true"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn profile_agent_survives_create() {
        let dir = temp_repo("profile-agent");
        let (record, _) =
            create(&dir, "task-profile", "codex:akun2", "do it", None, false).unwrap();
        assert_eq!(record.agent, "codex:akun2");
        assert!(create(&dir, "task-bad", "codex:Akun", "x", None, false).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prompt_file_role_parent_and_paths_land_in_the_brief() {
        let dir = temp_repo("options");
        let brief = dir.join("brief.md");
        fs::write(&brief, "long task from a file\n").unwrap();
        let role = dir.join("role.md");
        fs::write(&role, "# Role\nYou are the builder.\n").unwrap();
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "tugas-a".into(),
                agent: "codex".into(),
                prompt: "extra detail".into(),
                prompt_file: Some(brief),
                role: Some(role.to_string_lossy().into_owned()),
                parent: Some("orchestrator".into()),
                paths: vec!["src/**".into()],
                mode: Some("plan".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let text = fs::read_to_string(&record.brief).unwrap();
        assert!(text.contains("long task from a file"));
        assert!(text.contains("extra detail"));
        assert!(text.contains("You are the builder."));
        assert!(text.contains("- Parent: orchestrator"));
        assert!(text.contains("src/**"));
        assert!(text.contains(&format!("omnix handoff report {}", record.name)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn report_and_close_round_trip() {
        let dir = temp_repo("report");
        let (mut record, _) =
            create(&dir, "lapor", "codex", "do it", None, false).unwrap();
        mark_delivered(&mut record, "ho:lapor").unwrap();
        let reported = report(&dir, "lapor", "blocked", "waiting for DNS").unwrap();
        assert_eq!(reported.status.as_deref(), Some("blocked"));
        assert!(Path::new(reported.report.as_deref().unwrap()).exists());
        let closed = close_in(&dir, "lapor").unwrap();
        assert!(closed.closed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn close_refuses_a_dirty_worktree() {
        let dir = temp_repo("dirty");
        let (record, _) = create(&dir, "kotor", "codex", "edit", None, false).unwrap();
        fs::write(Path::new(&record.worktree).join("dirty.txt"), "x\n").unwrap();
        assert!(close_in(&dir, "kotor").is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handoff_leases_resources_then_close_releases_them() {
        let dir = temp_repo("locks");
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "terkunci".into(),
                agent: "codex".into(),
                prompt: "do it".into(),
                paths: vec!["src/**".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let worktree = format!("worktree:{}", record.worktree);
        let branch = format!("branch:{}", record.branch);
        assert_eq!(record.locks.len(), 3, "{:?}", record.locks);
        let held: Vec<String> = crate::locks::list(&dir)
            .into_iter()
            .map(|lock| lock.resource)
            .collect();
        assert!(held.contains(&worktree), "{held:?}");
        assert!(held.contains(&branch), "{held:?}");
        assert!(held.contains(&"path:src/**".to_string()), "{held:?}");

        let closed = close_in(&dir, "terkunci").unwrap();
        assert!(closed.closed);
        assert!(
            crate::locks::list(&dir).is_empty(),
            "close must release the handoff leases"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handoff_refuses_a_worktree_leased_by_someone_else() {
        let dir = temp_repo("lock-conflict");
        let target = dir.join("custom-worktree");
        let resource = format!("worktree:{}", target.display());
        crate::locks::acquire_as(
            &dir,
            &resource,
            "agent:lain",
            600,
            std::process::id(),
            BTreeMap::new(),
        )
        .unwrap();
        let err = create_with(
            &dir,
            CreateOptions {
                name: "bentrok".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                worktree_dir: Some(target.clone()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("agent:lain"), "{err}");
        assert!(!target.exists(), "no worktree may be created while leased");
        crate::locks::release(&dir, &resource, None, true).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn on_quota_parser_supports_stop_wait_and_fallback_list() {
        let dir = temp_repo("on-quota");
        assert_eq!(parse_on_quota(&dir, None).unwrap(), QuotaPolicy::Stop);
        assert_eq!(parse_on_quota(&dir, Some("stop")).unwrap(), QuotaPolicy::Stop);
        assert_eq!(parse_on_quota(&dir, Some("wait")).unwrap(), QuotaPolicy::Wait);
        let fb = parse_on_quota(
            &dir,
            Some("fallback=antigravity:gemini-4-neon-high,codex:gpt-6.2-sol"),
        )
        .unwrap();
        assert_eq!(
            fb,
            QuotaPolicy::Fallback(vec![
                FallbackTarget {
                    agent: "antigravity".into(),
                    model: Some("gemini-4-neon-high".into()),
                },
                FallbackTarget {
                    agent: "codex".into(),
                    model: Some("gpt-6.2-sol".into()),
                },
            ])
        );
        assert!(parse_on_quota(&dir, Some("bogus")).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_wip_and_report_commits_dirty_worktree_and_writes_blocked_report() {
        let dir = temp_repo("wip-quota");
        let (mut record, _) = create_with(
            &dir,
            CreateOptions {
                name: "kuotahabis".into(),
                agent: "antigravity".into(),
                model: Some("claude-sonnet-5-5-high".into()),
                prompt: "kerjakan".into(),
                ..Default::default()
            },
        )
        .unwrap();
        fs::write(Path::new(&record.worktree).join("wip.txt"), "progress\n").unwrap();
        let ex = crate::usage::Exhaustion {
            reset_at: Some(4_070_908_800),
            reset_local: Some("2099-01-01 07:00 WIB".into()),
            reset_text: Some("2099-01-01T00:00:00Z".into()),
            retryable: Some(false),
            confidence: "high",
        };
        let out = save_wip_and_report(
            &mut record,
            "antigravity",
            Some("claude-sonnet-5-5-high"),
            Some("ho:kuotahabis"),
            "Your AI credits balance is too low",
            &ex,
            false,
        )
        .unwrap();
        let sha = out.wip_sha.expect("wip sha");
        assert!(!sha.is_empty());
        assert_eq!(record.status.as_deref(), Some("blocked"));
        assert_eq!(record.wip_commit.as_deref(), Some(sha.as_str()));
        let report_body = fs::read_to_string(record.report.as_deref().unwrap()).unwrap();
        assert!(report_body.contains("Your AI credits balance is too low"));
        assert!(report_body.contains("wip.txt"));
        assert!(report_body.contains(&sha));
        assert!(git_dirty_files(Path::new(&record.worktree)).is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn launch_preview_includes_model_and_effort_args() {
        let dir = temp_repo("preview-argv");
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "ujiargv".into(),
                agent: "codex".into(),
                model: Some("gpt-6.2-sol".into()),
                effort: Some("medium".into()),
                prompt: "do it".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let line = format_launch_preview(&dir, &record);
        assert!(line.contains("codex"), "{line}");
        assert!(line.contains("-m gpt-6.2-sol"), "{line}");
        assert!(line.contains("model_reasoning_effort=medium"), "{line}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn creating_a_handoff_in_tests_never_writes_the_real_event_log() {
        let dir = temp_repo("isolated");
        let events = crate::control::events_path();
        assert!(
            events.starts_with(std::env::temp_dir()),
            "tests must not write {}",
            events.display()
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn orchestrator_flag_is_kept_on_the_record() {
        let dir = temp_repo("orch");
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "boss".into(),
                agent: "claude".into(),
                prompt: "plan".into(),
                orchestrator: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(record.orchestrator);
        let saved: Handoff = serde_json::from_slice(&fs::read(record_path(&record)).unwrap()).unwrap();
        assert!(saved.orchestrator, "flag must survive the record file");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_started_worker_never_leaves_the_dry_run_receipt() {
        let dir = temp_repo("started");
        let (mut record, path) = create_with(
            &dir,
            CreateOptions {
                name: "started".into(),
                agent: "claude".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        mark_running(&mut record, std::process::id()).unwrap();
        assert_eq!(record.outcome, "working");
        assert!(!record.receipt.detail.contains("dry run"), "{}", record.receipt.detail);
        assert!(record.worker_pid.is_some());
        let saved: Handoff = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(!saved.receipt.detail.contains("dry run"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn queued_and_delivered_receipts_do_not_keep_the_dry_run_text() {
        let dir = temp_repo("receipt");
        let (mut record, _) = create_with(
            &dir,
            CreateOptions {
                name: "rcpt".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(record.receipt.detail.starts_with("dry run"));
        mark_queued(&mut record, "waiting for quota").unwrap();
        assert_eq!(record.outcome, "queued");
        assert!(!record.receipt.detail.contains("dry run"), "{}", record.receipt.detail);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn force_close_removes_a_leftover_dir_that_is_not_a_worktree() {
        let dir = temp_repo("stray");
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "stray".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        git(&dir, &["worktree", "remove", "--force", &record.worktree]).unwrap();
        fs::create_dir_all(&record.worktree).unwrap();
        fs::write(Path::new(&record.worktree).join("junk.txt"), "junk\n").unwrap();
        assert!(close_record(record.clone()).is_err());
        let closed = close_record_with(record, true).unwrap();
        assert!(closed.closed);
        assert!(!Path::new(&closed.worktree).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forget_drops_a_missing_index_entry_and_keeps_the_rest() {
        let home = std::env::temp_dir().join(format!("omnix-index-{}-{}", std::process::id(), now()));
        fs::create_dir_all(home.join(".omnix")).unwrap();
        let gone = home.join("gone.json");
        let kept = home.join("kept.json");
        fs::write(&kept, "{}").unwrap();
        let text = vec![gone.to_string_lossy().into_owned(), kept.to_string_lossy().into_owned()];
        fs::write(home.join(".omnix").join("handoffs.json"), serde_json::to_vec(&text).unwrap()).unwrap();
        let prev = std::env::var_os("OMNIX_HOME");
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        assert_eq!(missing_index_entries(), vec![gone.clone()]);
        forget(&gone).unwrap();
        assert!(missing_index_entries().is_empty());
        let left: Vec<String> = serde_json::from_slice(&fs::read(index_path()).unwrap()).unwrap();
        assert_eq!(left, vec![kept.to_string_lossy().into_owned()]);
        match prev {
            Some(v) => unsafe { std::env::set_var("OMNIX_HOME", v) },
            None => unsafe { std::env::remove_var("OMNIX_HOME") },
        }
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn prune_orphans_drops_records_whose_repo_is_gone() {
        let dir = temp_repo("orphan");
        let (record, _) = create_with(
            &dir,
            CreateOptions {
                name: "orphan".into(),
                agent: "codex".into(),
                prompt: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let removed = prune_orphans(&[record.clone()]);
        assert_eq!(removed, vec![record.name.clone()]);
        assert!(prune_orphans(&[Handoff { closed: true, ..record }]).is_empty());
    }
}
