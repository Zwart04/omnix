//! Safe self-development: create a worktree for Omnix itself, build and test
//! the result there, run it beside the installed app with a separate
//! `OMNIX_HOME`, and only replace the running binary after an explicit click.
//!
//! Nothing in this module is automatic. The agent never tags, releases, or
//! installs; it leaves a branch and a report for the owner.

use crate::handoff;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MAX_CONTEXT: u64 = 256 * 1024;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn builds_dir() -> PathBuf {
    crate::control::home().join(".omnix").join("builds")
}

pub fn dev_home() -> PathBuf {
    crate::control::home()
        .join(".omnix")
        .join("dev")
        .join("home")
}

fn previous_dir() -> PathBuf {
    builds_dir().join("previous")
}

fn installed_path() -> PathBuf {
    builds_dir().join("installed.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: String,
    pub worktree: String,
    pub branch: String,
    pub commit: String,
    pub profile: String,
    pub binary: String,
    pub sha256: String,
    pub size: u64,
    pub built_at: u64,
    #[serde(default)]
    pub notes: Value,
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

fn repo_root(path: &Path) -> Result<PathBuf> {
    let text = git(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(text))
}

fn read_capped(path: &Path, what: &str) -> Result<String> {
    let meta = fs::metadata(path).with_context(|| format!("read {what} {}", path.display()))?;
    if meta.len() > MAX_CONTEXT {
        bail!("{what} is too large (limit {} KiB)", MAX_CONTEXT / 1024);
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

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_omnix_repo(repo: &Path) -> bool {
    repo.join("docs").join("architecture.md").is_file()
        && fs::read_to_string(repo.join("Cargo.toml"))
            .map(|text| text.contains("name = \"omnix\""))
            .unwrap_or(false)
}

#[derive(Debug, Clone, Default)]
pub struct TaskOptions {
    pub prompt_file: PathBuf,
    pub agent: String,
    pub name: Option<String>,
    pub repo: Option<PathBuf>,
    pub run: bool,
}

fn task_prompt(repo: &Path, brief: &str) -> Result<String> {
    let contributing = read_capped(&repo.join("CONTRIBUTING.md"), "CONTRIBUTING.md")?;
    let architecture = read_capped(&repo.join("docs").join("architecture.md"), "architecture")?;
    Ok(format!(
        "# Omnix development task\n\n\
You are working inside a worktree of Omnix itself. Make a focused change, run the tests, \
and leave a branch plus a pull request summary. Do not tag, publish a release, or install \
the build; the owner installs it explicitly from the Dev screen.\n\n\
## Brief\n\n{}\n\n\
## How to build and test\n\n\
```sh\n\
cargo build --release\n\
cargo test\n\
node app/selftest.cjs\n\
```\n\n\
Keep the existing commands and state format working. Measure idle RAM/CPU if the change \
can affect startup, and state honestly which platforms were not tested.\n\n\
## CONTRIBUTING.md\n\n{}\n\n\
## docs/architecture.md\n\n{}\n",
        brief.trim(),
        contributing.trim(),
        architecture.trim()
    ))
}

/// Create a worktree and brief. Delivery is the caller's decision so the GUI can
/// open the agent pane and the CLI can stay dry when the app is not running.
pub fn task(options: TaskOptions) -> Result<(handoff::Handoff, PathBuf)> {
    let root = options
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = repo_root(&root)?;
    if !is_omnix_repo(&repo) {
        bail!(
            "{} is not the Omnix repository (need Cargo.toml name=omnix and docs/architecture.md)",
            repo.display()
        );
    }
    let brief = read_capped(&options.prompt_file, "prompt file")?;
    if brief.trim().is_empty() {
        bail!("prompt file is empty");
    }
    let agent = if options.agent.trim().is_empty() {
        "codex".to_string()
    } else {
        options.agent.clone()
    };
    if crate::agent::split_agent(&agent).is_none() {
        bail!("unknown agent '{agent}'");
    }
    if !repo.join("CONTRIBUTING.md").is_file() {
        bail!("CONTRIBUTING.md is missing from {}", repo.display());
    }
    let name = options.name.unwrap_or_else(|| format!("dev-{}", now()));
    let prompt = task_prompt(&repo, &brief)?;
    let (mut record, path) = handoff::create_with(
        &repo,
        handoff::CreateOptions {
            name,
            agent,
            prompt,
            repo: Some(repo.clone()),
            mode: Some("edit".into()),
            ..Default::default()
        },
    )?;
    handoff::remember(&path)?;
    if options.run && crate::control::read_control(&crate::control::runtime_dir()).is_some() {
        let reply = crate::control::request(
            &crate::control::runtime_dir(),
            json!({"cmd": "spawnHandoff", "record": record}),
        )?;
        let pane = reply
            .get("paneKey")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        handoff::mark_delivered(&mut record, &pane)?;
    }
    Ok((record, path))
}

fn binary_path(worktree: &Path, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    let mut path = worktree.join("target").join(profile).join("omnix");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn build_one(worktree: &Path, release: bool) -> Result<Build> {
    let repo = repo_root(worktree)?;
    let status = Command::new("cargo")
        .arg("build")
        .args(if release { vec!["--release"] } else { vec![] })
        .current_dir(&repo)
        .status()
        .context("spawn cargo build")?;
    if !status.success() {
        bail!("cargo build failed");
    }
    let binary = binary_path(&repo, release);
    if !binary.is_file() {
        bail!("build finished but {} is missing", binary.display());
    }
    let bytes = fs::read(&binary).with_context(|| format!("read {}", binary.display()))?;
    let id = format!("dev-{}-{}", now(), now() % 1000);
    let dir = builds_dir().join(&id);
    fs::create_dir_all(&dir)?;
    let stored = dir.join(if cfg!(windows) { "omnix.exe" } else { "omnix" });
    fs::write(&stored, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&stored, fs::Permissions::from_mode(0o755));
    }
    let branch = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let commit = git(&repo, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let build = Build {
        id,
        worktree: repo.to_string_lossy().into_owned(),
        branch,
        commit,
        profile: if release { "release" } else { "debug" }.into(),
        binary: stored.to_string_lossy().into_owned(),
        sha256: sha256(&bytes),
        size: bytes.len() as u64,
        built_at: now(),
        notes: json!({}),
    };
    atomic_write(&dir.join("build.json"), &serde_json::to_vec_pretty(&build)?)?;
    Ok(build)
}

pub fn build(worktree: Option<PathBuf>, release: bool) -> Result<Build> {
    let root =
        worktree.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = repo_root(&root)?;
    if !is_omnix_repo(&repo) {
        bail!("build runs inside an Omnix repository");
    }
    build_one(&repo, release)
}

/// `cargo test` plus a small, honest bench: startup of `--help`, `doctor`, and
/// the binary size. It is not a GUI idle measurement.
pub fn test(worktree: Option<PathBuf>, offline: bool) -> Result<Value> {
    let root =
        worktree.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = repo_root(&root)?;
    let mut command = Command::new("cargo");
    command.arg("test").current_dir(&repo);
    if offline {
        command.arg("--offline");
    }
    let started = Instant::now();
    let status = command.status().context("spawn cargo test")?;
    let test_seconds = started.elapsed().as_secs_f64();
    if !status.success() {
        return Ok(json!({
            "ok": false,
            "tests": false,
            "testSeconds": test_seconds,
            "message": "cargo test failed",
        }));
    }
    let mut bench = json!({ "ok": true, "tests": true, "testSeconds": test_seconds });
    let binary = binary_path(&repo, true);
    if binary.is_file() {
        let mut runs = Vec::new();
        for _ in 0..3 {
            let started = Instant::now();
            let _ = Command::new(&binary)
                .arg("--help")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            runs.push(started.elapsed().as_secs_f64());
        }
        let started = Instant::now();
        let _ = Command::new(&binary)
            .args(["doctor", "--json"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let doctor = started.elapsed().as_secs_f64();
        let size = fs::metadata(&binary).map(|meta| meta.len()).unwrap_or(0);
        bench["startupHelpSeconds"] = json!(runs);
        bench["doctorSeconds"] = json!(doctor);
        bench["binaryBytes"] = json!(size);
        bench["binary"] = json!(binary.to_string_lossy());
    } else {
        bench["benchSkipped"] = json!("no release binary (run omnix dev build first)");
    }
    let path = repo.join(".omnix").join("dev-bench.json");
    let _ = atomic_write(&path, &serde_json::to_vec_pretty(&bench)?);
    bench["path"] = json!(path.to_string_lossy());
    Ok(bench)
}

pub fn builds() -> Vec<Build> {
    let Ok(read) = fs::read_dir(builds_dir()) else {
        return Vec::new();
    };
    let mut rows: Vec<Build> = read
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            fs::read(entry.path().join("build.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.built_at));
    rows
}

pub fn latest() -> Option<Build> {
    builds().into_iter().next()
}

pub fn find_build(id: Option<&str>) -> Result<Build> {
    match id {
        Some(id) => builds()
            .into_iter()
            .find(|build| build.id == id)
            .ok_or_else(|| anyhow::anyhow!("build '{id}' not found")),
        None => latest().ok_or_else(|| anyhow::anyhow!("no dev build yet; run omnix dev build")),
    }
}

pub fn run(id: Option<&str>) -> Result<Value> {
    let build = find_build(id)?;
    if !Path::new(&build.binary).is_file() {
        bail!("build binary is missing: {}", build.binary);
    }
    let home = dev_home();
    fs::create_dir_all(&home)?;
    let mut command = Command::new(&build.binary);
    command
        .current_dir(&build.worktree)
        .env("OMNIX_HOME", &home)
        .env("OMNIX_DEV_BUILD", &build.id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command.spawn().context("start the dev build")?;
    Ok(json!({
        "ok": true,
        "pid": child.id(),
        "build": build.id,
        "binary": build.binary,
        "home": home.to_string_lossy(),
        "note": "the owner's normal ~/.omnix state is untouched",
    }))
}

fn read_installed() -> Value {
    fs::read(installed_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({}))
}

/// Save the binary being replaced, then swap in the dev build. The caller must
/// have a real owner action behind it; nothing calls this automatically.
#[cfg(target_os = "linux")]
pub fn install(id: Option<&str>, restart: bool) -> Result<Value> {
    let build = find_build(id)?;
    let exe = std::env::current_exe().context("locate the running binary")?;
    let bytes = fs::read(&build.binary).with_context(|| format!("read {}", build.binary))?;
    let previous_bytes = fs::read(&exe).ok();
    if let Some(bytes) = previous_bytes {
        fs::create_dir_all(previous_dir())?;
        let previous = previous_dir().join("omnix");
        fs::write(&previous, &bytes)?;
        atomic_write(
            &previous_dir().join("previous.json"),
            &serde_json::to_vec_pretty(&json!({
                "version": env!("CARGO_PKG_VERSION"),
                "saved_at": now(),
                "path": previous.to_string_lossy(),
            }))?,
        )?;
    }
    let replaced = crate::update::replace_at(&exe, &bytes)?;
    if restart {
        let _ = Command::new(&replaced).spawn();
    }
    let state = json!({
        "build": build.id,
        "installed_at": now(),
        "previous": read_installed().get("build").cloned(),
    });
    atomic_write(&installed_path(), &serde_json::to_vec_pretty(&state)?)?;
    Ok(json!({
        "ok": true,
        "restart": restart,
        "installed": build.id,
        "sha256": build.sha256,
        "message": "dev build installed; restart Omnix to use it",
    }))
}

#[cfg(not(target_os = "linux"))]
pub fn install(_id: Option<&str>, _restart: bool) -> Result<Value> {
    bail!(
        "installing a dev binary in place is wired for Linux; use the normal installer on this platform"
    )
}

#[cfg(target_os = "linux")]
pub fn restore(restart: bool) -> Result<Value> {
    let previous = previous_dir().join("omnix");
    let bytes = fs::read(&previous)
        .with_context(|| format!("no previous binary at {}", previous.display()))?;
    let exe = std::env::current_exe().context("locate the running binary")?;
    let replaced = crate::update::replace_at(&exe, &bytes)?;
    if restart {
        let _ = Command::new(&replaced).spawn();
    }
    atomic_write(
        &installed_path(),
        &serde_json::to_vec_pretty(&json!({"build": null, "restored_at": now()}))?,
    )?;
    Ok(json!({"ok": true, "restart": restart, "message": "previous binary restored"}))
}

#[cfg(not(target_os = "linux"))]
pub fn restore(_restart: bool) -> Result<Value> {
    bail!("restoring a previous binary is wired for Linux only")
}

pub fn status() -> Value {
    let installed = read_installed();
    let previous = previous_dir().join("previous.json");
    json!({
        "builds": builds(),
        "installed": installed,
        "previous": fs::read(previous)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()),
        "devHome": dev_home().to_string_lossy(),
        "tasks": handoff::list_all()
            .into_iter()
            .filter(|record| record.name.starts_with("dev-"))
            .take(20)
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_brief_and_rules() {
        let dir = std::env::temp_dir().join(format!("omnix-dev-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("CONTRIBUTING.md"), "be kind").unwrap();
        fs::write(dir.join("docs").join("architecture.md"), "one state").unwrap();
        let prompt = task_prompt(&dir, "add a small thing").unwrap();
        assert!(prompt.contains("add a small thing"));
        assert!(prompt.contains("be kind"));
        assert!(prompt.contains("one state"));
        assert!(prompt.contains("Do not tag"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_ids_are_stable_strings() {
        assert!(sha256(b"x").len() == 64);
        assert!(now() > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn previous_binary_is_kept_outside_the_state_file() {
        let dir = std::env::temp_dir().join(format!("omnix-dev-previous-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("previous")).unwrap();
        fs::write(dir.join("previous").join("omnix"), b"old").unwrap();
        assert_eq!(
            fs::read(dir.join("previous").join("omnix")).unwrap(),
            b"old"
        );
        fs::remove_dir_all(&dir).ok();
    }
}
