//! Preview dev servers started from `.claude/launch.json`.
//!
//! Nothing here runs on a timer. `preview start` launches one detached process
//! group, waits for its port, and exits; `preview stop` (or app shutdown and
//! `omnix sleep`) kills that group. Logs and state live under
//! `.omnix/previews/` in the workspace, never in the global handoff index.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    pub name: String,
    pub runtime_executable: String,
    pub runtime_args: Vec<String>,
    pub port: u16,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewState {
    name: String,
    pid: u32,
    port: u16,
    url: String,
    log: String,
    started_at: u64,
    keep: bool,
    /// Lease id for `port:<n>`; old state files do not have one.
    #[serde(default)]
    lock: Option<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn preview_dir(root: &Path) -> PathBuf {
    root.join(".omnix").join("previews")
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 48 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "preview".into()
    } else {
        trimmed
    }
}

fn state_path(root: &Path, name: &str) -> PathBuf {
    preview_dir(root).join(format!("{}.json", slug(name)))
}

fn log_path(root: &Path, name: &str) -> PathBuf {
    preview_dir(root).join(format!("{}.log", slug(name)))
}

fn private_file(path: &Path) -> Result<fs::File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = fs::File::create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(file)
}

fn write_state(root: &Path, state: &PreviewState) -> Result<()> {
    let path = state_path(root, &state.name);
    let mut file = private_file(&path)?;
    use std::io::Write;
    file.write_all(serde_json::to_vec_pretty(state)?.as_slice())?;
    Ok(())
}

fn read_state(path: &Path) -> Option<PreviewState> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Load every configuration, accepting both the documented
/// `{ "configurations": [...] }` wrapper and a bare array.
pub fn load_launch(root: &Path) -> Result<Vec<LaunchConfig>> {
    let path = root.join(".claude").join("launch.json");
    let bytes = fs::read(&path).with_context(|| {
        format!(
            "no preview config at {} (add .claude/launch.json)",
            path.display()
        )
    })?;
    let value: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let list = value
        .get("configurations")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .with_context(|| format!("{} needs a configurations array", path.display()))?;
    let mut out = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .with_context(|| format!("launch configuration {index} has no name"))?
            .to_string();
        let runtime_executable = item
            .get("runtimeExecutable")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|exe| !exe.is_empty())
            .with_context(|| format!("preview '{name}' has no runtimeExecutable"))?
            .to_string();
        let runtime_args = item
            .get("runtimeArgs")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(|arg| match arg {
                        Value::String(text) => Some(text.clone()),
                        Value::Number(number) => Some(number.to_string()),
                        Value::Bool(flag) => Some(flag.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let port = match item.get("port") {
            Some(Value::Number(number)) => number
                .as_u64()
                .and_then(|port| u16::try_from(port).ok())
                .with_context(|| format!("preview '{name}' has an invalid port"))?,
            Some(Value::String(text)) => text
                .parse::<u16>()
                .with_context(|| format!("preview '{name}' has an invalid port"))?,
            _ => bail!("preview '{name}' has no port"),
        };
        if port == 0 {
            bail!("preview '{name}' has an invalid port");
        }
        let cwd = item.get("cwd").and_then(Value::as_str).map(str::to_string);
        let mut env = BTreeMap::new();
        if let Some(map) = item.get("env").and_then(Value::as_object) {
            for (key, value) in map {
                let text = match value {
                    Value::String(text) => text.clone(),
                    Value::Number(number) => number.to_string(),
                    Value::Bool(flag) => flag.to_string(),
                    _ => continue,
                };
                env.insert(key.clone(), text);
            }
        }
        out.push(LaunchConfig {
            name,
            runtime_executable,
            runtime_args,
            port,
            cwd,
            env,
        });
    }
    if out.is_empty() {
        bail!("{} has no preview configurations", path.display());
    }
    Ok(out)
}

fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

/// A preview may not take a port that is busy or leased by another preview.
/// Scanning starts just above the configured port so the default keeps
/// working, and falls back to an OS-assigned port when the range is crowded.
fn pick_port(root: &Path, start: u16) -> Result<u16> {
    let last = start.saturating_add(100);
    for port in start.saturating_add(1)..=last {
        if port == 0 || port_open(port) {
            continue;
        }
        if crate::locks::get(root, &format!("port:{port}")).is_some() {
            continue;
        }
        return Ok(port);
    }
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("ask the OS for a free preview port")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

/// Point explicit `--port` arguments at the chosen port when they carried the
/// configured one. Everything else is left exactly as the user wrote it.
fn with_port(args: &[String], from: u16, to: u16) -> Vec<String> {
    let old = from.to_string();
    let new = to.to_string();
    let mut out = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if (arg == "--port" || arg == "-p") && args.get(index + 1) == Some(&old) {
            out.push(arg.clone());
            out.push(new.clone());
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--port=") {
            if value == old {
                out.push(format!("--port={new}"));
                index += 1;
                continue;
            }
        }
        if let Some(value) = arg.strip_prefix("-p=") {
            if value == old {
                out.push(format!("-p={new}"));
                index += 1;
                continue;
            }
        }
        out.push(arg.clone());
        index += 1;
    }
    out
}

fn kill_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        // The child was started with its own process group, so the negative
        // PID reaches npm/node/vite children too.
        let group = format!("-{pid}");
        let _ = Command::new("kill")
            .args(["-TERM", "--", &group])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_millis(600));
        let _ = Command::new("kill")
            .args(["-KILL", "--", &group])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn tail(path: &Path, lines: usize) -> String {
    let mut text = String::new();
    if let Ok(mut file) = fs::File::open(path) {
        let _ = file.read_to_string(&mut text);
    }
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

fn wait_ready(child: &mut Child, port: u16, log: &Path) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(25) {
        if let Some(status) = child.try_wait()? {
            let detail = tail(log, 20);
            bail!("preview exited with {status} before port {port} was ready\n{detail}");
        }
        if port_open(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    bail!(
        "preview did not open 127.0.0.1:{port} within 25s\n{}",
        tail(log, 20)
    )
}

fn find_config(root: &Path, name: &str) -> Result<LaunchConfig> {
    let configs = load_launch(root)?;
    configs
        .into_iter()
        .find(|config| config.name == name)
        .with_context(|| {
            let names: Vec<String> = load_launch(root)
                .map(|list| list.into_iter().map(|c| c.name).collect())
                .unwrap_or_default();
            format!(
                "no preview '{name}' in .claude/launch.json (have: {})",
                names.join(", ")
            )
        })
}

fn read_all_states(root: &Path) -> Vec<(PathBuf, PreviewState)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(preview_dir(root)) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(state) = read_state(&path) {
            out.push((path, state));
        }
    }
    out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    out
}

fn locate(root: &Path, name: &str) -> Option<(PathBuf, PreviewState)> {
    let wanted = slug(name);
    read_all_states(root).into_iter().find(|(path, state)| {
        state.name == name || path.file_stem().and_then(|s| s.to_str()) == Some(wanted.as_str())
    })
}

pub fn start(root: &Path, name: &str, keep: bool) -> Result<Value> {
    let config = find_config(root, name)?;
    if let Some((path, state)) = locate(root, name) {
        if alive(state.pid) && port_open(state.port) {
            return Ok(json!({
                "alreadyRunning": true,
                "name": state.name,
                "pid": state.pid,
                "port": state.port,
                "url": state.url,
                "log": state.log,
            }));
        }
        let _ = fs::remove_file(path);
    }

    let configured_port = config.port;
    let mut port = configured_port;
    if port_open(port) || crate::locks::get(root, &format!("port:{port}")).is_some() {
        port = pick_port(root, configured_port)?;
    }

    let log_path = log_path(root, &config.name);
    let log = private_file(&log_path)?;
    let cwd = match config.cwd.as_deref() {
        Some(path) => root.join(path),
        None => root.to_path_buf(),
    };
    let args = if port == configured_port {
        config.runtime_args.clone()
    } else {
        with_port(&config.runtime_args, configured_port, port)
    };
    let owner = format!("preview:{}", config.name);
    let resource = format!("port:{port}");
    let mut meta = BTreeMap::new();
    meta.insert("preview".into(), config.name.clone());
    meta.insert("url".into(), format!("http://127.0.0.1:{port}"));
    let mut lock = crate::locks::acquire_with(
        root,
        &resource,
        &owner,
        crate::locks::DEFAULT_TTL,
        meta.clone(),
    )?;

    let mut cmd = Command::new(&config.runtime_executable);
    cmd.args(&args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    for (key, value) in &config.env {
        cmd.env(key, value);
    }
    if port != configured_port && !config.env.contains_key("PORT") {
        // Most dev servers that do not hardcode --port read PORT.
        cmd.env("PORT", port.to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd
        .spawn()
        .with_context(|| format!("start preview '{}'", config.name))
    {
        Ok(child) => child,
        Err(err) => {
            let _ = crate::locks::release(root, &lock.id, None, true);
            return Err(err);
        }
    };
    // Bind the lease to the server process so an abandoned preview expires
    // without a background monitor.
    match crate::locks::acquire_as(
        root,
        &resource,
        &owner,
        crate::locks::DEFAULT_TTL,
        child.id(),
        meta,
    ) {
        Ok(renewed) => lock = renewed,
        Err(err) => {
            kill_tree(child.id());
            let _ = crate::locks::release(root, &lock.id, None, true);
            return Err(err);
        }
    }
    let url = format!("http://127.0.0.1:{port}");
    let state = PreviewState {
        name: config.name.clone(),
        pid: child.id(),
        port,
        url: url.clone(),
        log: log_path.to_string_lossy().to_string(),
        started_at: now(),
        keep,
        lock: Some(lock.id.clone()),
    };
    if let Err(err) = write_state(root, &state) {
        kill_tree(state.pid);
        let _ = crate::locks::release(root, &lock.id, None, true);
        return Err(err);
    }
    if let Err(err) = wait_ready(&mut child, port, &log_path) {
        kill_tree(state.pid);
        let _ = fs::remove_file(state_path(root, &state.name));
        let _ = crate::locks::release(root, &lock.id, None, true);
        return Err(err);
    }
    drop(child);

    // The CLI must keep working without the app; a failed open is not fatal.
    let _ = crate::control::request(
        &crate::control::runtime_dir(),
        json!({ "cmd": "openUrl", "url": url.clone() }),
    );
    crate::control::event(
        "preview-started",
        json!({
            "name": state.name,
            "pid": state.pid,
            "port": state.port,
            "configuredPort": configured_port,
            "autoPort": port != configured_port,
            "lock": state.lock,
            "keep": keep,
        }),
    );
    Ok(json!({
        "name": state.name,
        "pid": state.pid,
        "port": state.port,
        "configuredPort": configured_port,
        "autoPort": port != configured_port,
        "url": state.url,
        "log": state.log,
        "lock": state.lock,
        "keep": keep,
    }))
}

pub fn stop(root: &Path, name: &str) -> Result<Value> {
    let Some((path, state)) = locate(root, name) else {
        bail!(
            "no preview named '{name}' in {}",
            preview_dir(root).display()
        );
    };
    kill_tree(state.pid);
    let _ = fs::remove_file(&path);
    if let Some(id) = &state.lock {
        let _ = crate::locks::release(root, id, None, true);
    }
    crate::control::event(
        "preview-stopped",
        json!({ "name": state.name, "pid": state.pid, "port": state.port, "lock": state.lock }),
    );
    Ok(json!({
        "name": state.name,
        "pid": state.pid,
        "port": state.port,
        "stopped": !alive(state.pid),
    }))
}

pub fn logs(root: &Path, name: &str, errors_only: bool) -> Result<String> {
    let Some((_, state)) = locate(root, name) else {
        bail!(
            "no preview named '{name}' in {}",
            preview_dir(root).display()
        );
    };
    let text = fs::read_to_string(&state.log)
        .with_context(|| format!("read preview log {}", state.log))?;
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            if !errors_only {
                return true;
            }
            let lower = line.to_ascii_lowercase();
            ["error", "warn", "fail", "exception", "panic", "traceback"]
                .iter()
                .any(|needle| lower.contains(needle))
        })
        .collect();
    let start = lines.len().saturating_sub(200);
    let body = lines[start..].join("\n");
    if body.is_empty() {
        Ok(if errors_only {
            "(no error lines)".into()
        } else {
            "(empty log)".into()
        })
    } else {
        Ok(body)
    }
}

pub fn list(root: &Path) -> Vec<Value> {
    read_all_states(root)
        .into_iter()
        .map(|(_, state)| {
            json!({
                "name": state.name,
                "pid": state.pid,
                "port": state.port,
                "url": state.url,
                "log": state.log,
                "startedAt": state.started_at,
                "keep": state.keep,
                "alive": alive(state.pid),
                "ready": port_open(state.port),
            })
        })
        .collect()
}

/// Called from app shutdown and `omnix sleep`. `include_keep` lets `gc` also
/// stop servers explicitly marked `--keep`.
pub fn stop_all(root: &Path, include_keep: bool) -> Vec<String> {
    let mut stopped = Vec::new();
    for (path, state) in read_all_states(root) {
        if state.keep && !include_keep {
            continue;
        }
        kill_tree(state.pid);
        let _ = fs::remove_file(path);
        if let Some(id) = &state.lock {
            let _ = crate::locks::release(root, id, None, true);
        }
        stopped.push(state.name);
    }
    stopped
}

/// Remove state for a process that died on its own, so `preview list` stays
/// honest without a background monitor.
pub fn prune(root: &Path, dry: bool) -> Vec<String> {
    let mut actions = Vec::new();
    for (path, state) in read_all_states(root) {
        if alive(state.pid) {
            continue;
        }
        actions.push(format!("prune preview {}", state.name));
        if !dry {
            let _ = fs::remove_file(path);
            if let Some(id) = &state.lock {
                let _ = crate::locks::release(root, id, None, true);
            }
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-preview-{tag}-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".claude")).unwrap();
        dir
    }

    fn write_launch(root: &Path, body: &str) {
        fs::write(root.join(".claude").join("launch.json"), body).unwrap();
    }

    #[test]
    fn launch_parses_wrapped_and_bare_configs() {
        let root = temp_root("launch");
        write_launch(
            &root,
            r#"{"configurations":[
                {"name":"Web","runtimeExecutable":"npm","runtimeArgs":["run","dev"],"port":3000},
                {"name":"API","runtimeExecutable":"node","port":"4000","env":{"NODE_ENV":"dev"}}
            ]}"#,
        );
        let configs = load_launch(&root).unwrap();
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].name, "Web");
        assert_eq!(configs[0].runtime_args, vec!["run", "dev"]);
        assert_eq!(configs[0].port, 3000);
        assert_eq!(configs[1].port, 4000);
        assert_eq!(configs[1].env.get("NODE_ENV"), Some(&"dev".to_string()));

        write_launch(
            &root,
            r#"[{"name":"bare","runtimeExecutable":"npm","port":5173}]"#,
        );
        assert_eq!(load_launch(&root).unwrap()[0].name, "bare");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_rejects_missing_fields_and_bad_ports() {
        let root = temp_root("bad");
        write_launch(
            &root,
            r#"{"configurations":[{"name":"x","runtimeExecutable":"npm"}]}"#,
        );
        assert!(load_launch(&root).is_err());
        write_launch(
            &root,
            r#"{"configurations":[{"name":"x","runtimeExecutable":"npm","port":70000}]}"#,
        );
        assert!(load_launch(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn names_are_slugged_for_state_files() {
        assert_eq!(slug("Web Local"), "web-local");
        assert_eq!(slug("  "), "preview");
        assert!(slug(&"a".repeat(80)).len() <= 48);
    }

    #[test]
    fn logs_filter_errors_and_tail() {
        let root = temp_root("logs");
        write_launch(
            &root,
            r#"{"configurations":[{"name":"Web","runtimeExecutable":"npm","port":3000}]}"#,
        );
        let path = log_path(&root, "Web");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "booting\nTypeError: boom\nall good\n").unwrap();
        let state = PreviewState {
            name: "Web".into(),
            pid: 999_999,
            port: 3000,
            url: "http://127.0.0.1:3000".into(),
            log: path.to_string_lossy().into(),
            started_at: 0,
            keep: false,
            lock: None,
        };
        write_state(&root, &state).unwrap();
        let text = logs(&root, "Web", true).unwrap();
        assert!(text.contains("TypeError"));
        assert!(!text.contains("all good"));
        let list = list(&root);
        assert_eq!(list[0]["name"], "Web");
        assert_eq!(list[0]["alive"], false);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn auto_port_skips_leased_ports_and_rewrites_arguments() {
        let root = temp_root("ports");
        let owner = "agent:lain";
        crate::locks::acquire_as(
            &root,
            "port:3000",
            owner,
            600,
            std::process::id(),
            BTreeMap::new(),
        )
        .unwrap();
        let picked = pick_port(&root, 3000).unwrap();
        assert_ne!(picked, 3000, "the leased port must not be chosen");
        assert!(
            crate::locks::get(&root, &format!("port:{picked}")).is_none(),
            "picked port must be free"
        );

        let args: Vec<String> = ["run", "dev", "--port", "3000"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            with_port(&args, 3000, 3100),
            vec!["run", "dev", "--port", "3100"]
        );
        let eq: Vec<String> = ["--port=3000", "-p", "3000"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            with_port(&eq, 3000, 3100),
            vec!["--port=3100", "-p", "3100"]
        );
        // A different explicit port is the user's choice and stays untouched.
        let other: Vec<String> = ["--port", "4000"].iter().map(|s| s.to_string()).collect();
        assert_eq!(with_port(&other, 3000, 3100), other);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stop_releases_the_preview_port_lease() {
        let home = std::env::temp_dir().join(format!("omnix-preview-home-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        unsafe { std::env::set_var("OMNIX_HOME", &home) };
        let root = temp_root("lock-stop");
        let lock = crate::locks::acquire_as(
            &root,
            "port:5173",
            "preview:Web",
            600,
            std::process::id(),
            BTreeMap::new(),
        )
        .unwrap();
        let path = log_path(&root, "Web");
        let state = PreviewState {
            name: "Web".into(),
            pid: 999_999,
            port: 5173,
            url: "http://127.0.0.1:5173".into(),
            log: path.to_string_lossy().into(),
            started_at: 0,
            keep: false,
            lock: Some(lock.id),
        };
        write_state(&root, &state).unwrap();
        stop(&root, "Web").unwrap();
        assert!(crate::locks::list(&root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
