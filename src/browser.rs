use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Agent browser sessions outlive one `browser mcp` call only while that call
/// runs. The lease makes an orphaned session visible so another agent can see
/// who owns the browser without killing the shared daemon by accident.
pub const SESSION_TTL: u64 = 4 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub owner: String,
    pub pid: u32,
    pub created_at: u64,
    pub expires_at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn sessions_dir(root: &Path) -> PathBuf {
    root.join(".omnix").join("browser").join("sessions")
}

fn session_path(root: &Path, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(80)
        .collect();
    sessions_dir(root).join(format!("{}-{}.json", safe, now()))
}

fn read_session(path: &Path) -> Option<Session> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn session_active(session: &Session) -> bool {
    session.expires_at > now() && crate::locks::pid_alive(session.pid)
}

/// Register the caller before `browser mcp` starts. Reusing an id owned by the
/// same PID refreshes the lease; a live different PID keeps the id exclusive.
pub fn session_start(root: &Path, id: &str, owner: &str, pid: u32) -> Result<Session> {
    let id = id.trim();
    if id.is_empty() || id.len() > 120 || id.chars().any(char::is_control) {
        bail!("browser session id must be 1-120 printable characters");
    }
    for (_path, existing) in session_entries(root) {
        if existing.id == id && existing.pid != pid && session_active(&existing) {
            bail!(
                "browser session '{id}' is owned by {} (pid {})",
                existing.owner,
                existing.pid
            );
        }
    }
    let stamp = now();
    let session = Session {
        id: id.to_string(),
        owner: owner.chars().take(120).collect(),
        pid,
        created_at: stamp,
        expires_at: stamp.saturating_add(SESSION_TTL),
    };
    let dir = sessions_dir(root);
    std::fs::create_dir_all(&dir)?;
    // Replace any stale record with the same id first.
    for (path, existing) in session_entries(root) {
        if existing.id == id {
            let _ = std::fs::remove_file(path);
        }
    }
    let file = session_path(root, id);
    let mut handle = std::fs::File::create(&file)?;
    use std::io::Write;
    handle.write_all(serde_json::to_vec_pretty(&session)?.as_slice())?;
    Ok(session)
}

fn session_entries(root: &Path) -> Vec<(PathBuf, Session)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(sessions_dir(root)) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(session) = read_session(&path) {
            out.push((path, session));
        }
    }
    out
}

/// Live sessions, pruning expired leases and dead proxy PIDs as a side effect.
pub fn session_list(root: &Path) -> Vec<Session> {
    let mut out = Vec::new();
    for (path, session) in session_entries(root) {
        if session_active(&session) {
            out.push(session);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    out
}

pub fn session_close(root: &Path, id: &str) -> Result<Option<Session>> {
    for (path, session) in session_entries(root) {
        if session.id == id {
            std::fs::remove_file(&path)?;
            return Ok(Some(session));
        }
    }
    Ok(None)
}

pub fn env_session() -> Option<String> {
    env::var("OMNIX_BROWSER_SESSION")
        .ok()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

fn stop_pid(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Lightpanda,
    Native,
}

impl Engine {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "lightpanda" => Ok(Self::Lightpanda),
            "native" => Ok(Self::Native),
            _ => bail!("unknown browser engine '{s}' (native|lightpanda)"),
        }
    }
}

pub fn binary() -> String {
    env::var("OMNIX_LIGHTPANDA").unwrap_or_else(|_| "lightpanda".into())
}

/// Persistent native-browser profile: cookies/localStorage for the human tab.
/// The agent session never reads this directory directly.
pub fn native_data_dir() -> PathBuf {
    crate::control::home()
        .join(".omnix")
        .join("browser")
        .join("native")
}

/// Temporary cookie handoff written by `omnix browser session import`.
pub fn cookie_session_path(root: &std::path::Path) -> PathBuf {
    root.join(".omnix")
        .join("browser")
        .join("cookies")
        .join("session.json")
}

fn lightpanda(args: &[&str]) -> Result<std::process::ExitStatus> {
    Command::new(binary()).args(args).status().with_context(|| {
        format!(
            "could not run '{}'. Install Lightpanda or set OMNIX_LIGHTPANDA. See docs/browser.md",
            binary()
        )
    })
}

pub fn version() -> Option<String> {
    let out = Command::new(binary()).arg("version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn check_dump(dump: &str) -> Result<()> {
    if !matches!(dump, "markdown" | "html" | "semantic_tree_text") {
        bail!("--dump must be markdown, html, or semantic_tree_text");
    }
    Ok(())
}

/// Only web pages go to Lightpanda: a stray `file://` or bare path should not
/// turn a fetch into a local file read.
pub(crate) fn check_url(url: &str) -> Result<()> {
    let ok = url
        .split_once("://")
        .map(|(scheme, rest)| {
            matches!(scheme, "http" | "https") && !rest.is_empty() && !rest.starts_with('/')
        })
        .unwrap_or(false);
    if !ok {
        bail!("browser URL must start with http:// or https://");
    }
    Ok(())
}

/// Fetch a page and return its text. Used by the GUI text pane.
pub fn fetch_text(url: &str, dump: &str) -> Result<String> {
    check_dump(dump)?;
    check_url(url)?;
    let out = Command::new(binary())
        .args(["fetch", url, "--dump", dump])
        .output()
        .with_context(|| {
            format!(
                "could not run '{}'. Install Lightpanda or set OMNIX_LIGHTPANDA",
                binary()
            )
        })?;
    if !out.status.success() {
        bail!(
            "lightpanda fetch failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn fetch(url: &str, engine: Engine, dump: &str) -> Result<()> {
    match engine {
        Engine::Lightpanda => {
            check_dump(dump)?;
            check_url(url)?;
            let status = lightpanda(&["fetch", url, "--dump", dump])?;
            if !status.success() {
                bail!("lightpanda fetch exited with {status}");
            }
            Ok(())
        }
        Engine::Native => bail!(
            "the native engine is the app's embedded WebView. The CLI cannot draw it.\n\
             Wire the harness in docs/browser.md, or use --engine lightpanda for headless runs."
        ),
    }
}

pub fn serve(port: u16) -> Result<u16> {
    if let Some((pid, running_port)) = running() {
        println!(
            "lightpanda pid {pid}  cdp http://127.0.0.1:{running_port}\nconnect: ws://127.0.0.1:{running_port}\n(shared browser already running; stop it with `omnix browser stop`)"
        );
        return Ok(running_port);
    }
    let port_s = port.to_string();
    // Preflight the bind: a busy port makes Lightpanda exit before we can
    // record a PID, which used to look like a successful start.
    {
        use std::net::{Ipv4Addr, SocketAddr, TcpListener};
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        if let Err(err) = TcpListener::bind(addr) {
            bail!("127.0.0.1:{port} is not free ({err}); stop that process or pass another --port");
        }
    }
    let mut cmd = Command::new(binary());
    cmd.args([
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        &port_s,
        "--cdp-max-connections",
        "4",
        "--cdp-max-pending-connections",
        "8",
        "--remote-allow-origins",
        "http://localhost",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    // Own process group so a shell or terminal closing does not SIGHUP the
    // daemon. It outlives this CLI call on purpose.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = cmd.spawn().with_context(|| {
        format!(
            "could not start '{}'. Install Lightpanda or set OMNIX_LIGHTPANDA",
            binary()
        )
    })?;
    let pid = child.id();
    wait_until_listening(pid, port)?;
    if let Err(err) = write_state(pid, port) {
        // Do not leave a daemon the user cannot find or stop.
        let _ = kill(pid);
        return Err(err);
    }
    println!(
        "lightpanda pid {pid}  cdp http://127.0.0.1:{port}\nconnect: ws://127.0.0.1:{port}\nstop: omnix browser stop"
    );
    drop(child);
    Ok(port)
}

/// A PID alone is not proof the daemon is up: a busy port makes Lightpanda
/// exit immediately. Poll the socket so `serve` never prints a false success.
fn wait_until_listening(pid: u32, port: u16) -> Result<()> {
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    use std::time::{Duration, Instant};
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let start = Instant::now();
    loop {
        if !alive(pid) {
            bail!("lightpanda exited before listening on 127.0.0.1:{port}; is the port already in use?");
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            // The socket may belong to some other process; only trust it if
            // our child is still alive a moment later.
            std::thread::sleep(Duration::from_millis(100));
            if alive(pid) {
                return Ok(());
            }
            bail!("lightpanda exited while opening 127.0.0.1:{port}");
        }
        if start.elapsed() > Duration::from_secs(3) {
            let _ = kill(pid);
            bail!("lightpanda did not listen on 127.0.0.1:{port} within 3s");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn state_path() -> PathBuf {
    crate::root().join(".omnix").join("browser.json")
}

fn write_state(pid: u32, port: u16) -> Result<()> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&json!({ "pid": pid, "port": port }))?,
    )?;
    Ok(())
}

fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // Linux: a recycled PID must not make `stop` kill an unrelated process.
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        let Ok(mut file) = std::fs::File::open(format!("/proc/{pid}/comm")) else {
            return false;
        };
        let mut name = String::new();
        file.read_to_string(&mut name).is_ok() && name.trim_start().starts_with("lightpanda")
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).contains("lightpanda"))
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// The shared Lightpanda daemon, if it is still alive.
pub fn running() -> Option<(u32, u16)> {
    let bytes = std::fs::read(state_path()).ok()?;
    let state: Value = serde_json::from_slice(&bytes).ok()?;
    let pid = state.get("pid").and_then(Value::as_u64)? as u32;
    let port = state.get("port").and_then(Value::as_u64)? as u16;
    alive(pid).then_some((pid, port))
}

fn kill(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..20 {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        !alive(pid)
    }
    #[cfg(windows)]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// `browser stop` closes the caller's session only. `--all` is the explicit
/// escape hatch that stops the shared daemon; one agent must never be able to
/// free another agent's pages by accident.
pub fn stop(session: Option<&str>, all: bool) -> Result<Value> {
    if all {
        let path = state_path();
        let Some((pid, _)) = running() else {
            let _ = std::fs::remove_file(&path);
            for (file, _) in session_entries(&crate::root()) {
                let _ = std::fs::remove_file(file);
            }
            return Ok(json!({ "stopped": false, "sessions": 0 }));
        };
        if !kill(pid) {
            bail!("could not stop lightpanda pid {pid}");
        }
        let _ = std::fs::remove_file(&path);
        let mut closed = 0;
        for (file, _) in session_entries(&crate::root()) {
            if std::fs::remove_file(file).is_ok() {
                closed += 1;
            }
        }
        return Ok(json!({ "stopped": true, "pid": pid, "sessions": closed }));
    }

    let root = crate::root();
    let target = session
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(env_session);
    let Some(target) = target else {
        let active = session_list(&root);
        if active.len() == 1 {
            return close_session(&root, &active[0]);
        }
        if active.len() > 1 {
            bail!(
                "{} browser sessions are active; pass --session ID or --all",
                active.len()
            );
        }
        if let Some((pid, _)) = running() {
            bail!(
                "no browser session for this terminal; shared lightpanda pid {pid} is still running (use --all to stop it)"
            );
        }
        return Ok(json!({ "stopped": false, "sessions": 0 }));
    };
    for active in session_list(&root) {
        if active.id == target {
            return close_session(&root, &active);
        }
    }
    Ok(json!({ "stopped": false, "session": target }))
}

fn close_session(root: &Path, session: &Session) -> Result<Value> {
    let killed = stop_pid(session.pid);
    session_close(root, &session.id)?;
    Ok(json!({
        "stopped": true,
        "session": session.id,
        "owner": session.owner,
        "pid": session.pid,
        "agentStopped": killed,
        "daemon": running().map(|(pid, _)| pid),
    }))
}

/// Run Lightpanda's MCP server with bounded sessions. This is the browser tool
/// every agent should use; it never downloads or starts Chromium.
pub fn mcp(args: &[String]) -> Result<()> {
    // `--session` is Omnix's lease name, not a Lightpanda flag.
    let mut forwarded = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--session" {
            index += 2;
            continue;
        }
        if let Some(value) = args[index].strip_prefix("--session=") {
            let _ = value;
            index += 1;
            continue;
        }
        forwarded.push(args[index].clone());
        index += 1;
    }
    let explicit = args
        .iter()
        .position(|arg| arg == "--session")
        .and_then(|at| args.get(at + 1))
        .cloned()
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("--session=").map(str::to_string))
        });
    let session = explicit.or_else(env_session);
    let Some(id) = session else {
        // The proxy spawns Lightpanda itself, adds the native-review tools, and
        // enforces the optional host allowlist. `--raw` keeps the direct exec.
        return crate::mcp_proxy::run(&forwarded);
    };
    let root = crate::root();
    let owner = env::var("OMNIX_OWNER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("browser:{id}"));
    let started = session_start(&root, &id, &owner, std::process::id())?;
    let result = crate::mcp_proxy::run(&forwarded);
    let _ = session_close(&root, &started.id);
    result
}

pub fn print_sessions() -> Result<()> {
    let rows = session_list(&crate::root());
    if rows.is_empty() {
        println!("no active browser sessions");
    }
    for session in rows {
        println!(
            "{:<28} {:<28} pid {:<8} until {}",
            session.id,
            session.owner,
            session.pid,
            crate::control::local_time(session.expires_at)
        );
    }
    Ok(())
}

pub fn doctor() -> Result<()> {
    println!("engine: lightpanda");
    println!("binary: {}", binary());
    match version() {
        Some(text) => println!("version: {text}"),
        None => println!("version: not found (native app WebView is still usable)"),
    }
    match running() {
        Some((pid, port)) => println!("shared: pid {pid}  cdp http://127.0.0.1:{port}"),
        None => println!("shared: not running (start with `omnix browser serve`)"),
    }
    println!("note: Lightpanda does not paint pixels; captureScreenshot is a placeholder.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names_are_exact() {
        assert_eq!(Engine::parse("native").unwrap(), Engine::Native);
        assert_eq!(Engine::parse("lightpanda").unwrap(), Engine::Lightpanda);
        assert!(Engine::parse("chrome").is_err());
    }

    #[test]
    fn only_web_urls_reach_the_browser() {
        assert!(check_url("https://example.com/a?b=1").is_ok());
        assert!(check_url("http://127.0.0.1:3000").is_ok());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("javascript:alert(1)").is_err());
        assert!(check_url("https:///etc/passwd").is_err());
        assert!(check_url("example.com").is_err());
        assert!(check_url("").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn session_leases_are_exclusive_and_close_one_at_a_time() {
        let root = std::env::temp_dir().join(format!(
            "omnix-browser-sessions-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let me = std::process::id();
        let other = crate::locks::holder_pid();
        assert_ne!(other, me, "the test parent is a different live process");

        session_start(&root, "one", "agent:a", me).unwrap();
        session_start(&root, "two", "agent:b", me).unwrap();
        let err = session_start(&root, "one", "agent:b", other)
            .unwrap_err()
            .to_string();
        assert!(err.contains("agent:a"), "{err}");
        assert_eq!(session_list(&root).len(), 2);

        let closed = session_close(&root, "one").unwrap().unwrap();
        assert_eq!(closed.id, "one");
        let left = session_list(&root);
        assert_eq!(left.len(), 1, "only the named session is closed");
        assert_eq!(left[0].id, "two");
        session_close(&root, "two").unwrap();
        assert!(session_list(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn expired_session_leases_are_pruned_on_read() {
        let root = std::env::temp_dir().join(format!(
            "omnix-browser-expire-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let me = std::process::id();
        session_start(&root, "old", "agent:a", me).unwrap();
        let (file, mut session) = session_entries(&root).pop().unwrap();
        session.expires_at = 1;
        std::fs::write(&file, serde_json::to_vec(&session).unwrap()).unwrap();
        assert!(session_list(&root).is_empty(), "expired lease is pruned");
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
