use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
fn check_url(url: &str) -> Result<()> {
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

pub fn stop() -> Result<()> {
    let path = state_path();
    let Some((pid, _)) = running() else {
        let _ = std::fs::remove_file(&path);
        println!("no shared Lightpanda browser is running");
        return Ok(());
    };
    if kill(pid) {
        let _ = std::fs::remove_file(&path);
        println!("stopped lightpanda pid {pid}");
        Ok(())
    } else {
        bail!("could not stop lightpanda pid {pid}")
    }
}

/// Run Lightpanda's MCP server with bounded sessions. This is the browser tool
/// every agent should use; it never downloads or starts Chromium.
pub fn mcp(args: &[String]) -> Result<()> {
    let mut cmd = Command::new(binary());
    cmd.arg("mcp");
    cmd.args(args);
    let has_port = args
        .iter()
        .any(|a| a == "--port" || a.starts_with("--port="));
    let has_timeout = args.iter().any(|a| {
        a == "--http-session-timeout" || a.starts_with("--http-session-timeout=")
    });
    if has_port && !has_timeout {
        let seconds = env::var("OMNIX_BROWSER_IDLE_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(900)
            .to_string();
        cmd.args(["--http-session-timeout", seconds.as_str()]);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        bail!("could not start '{} mcp': {err}", binary());
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status().with_context(|| {
            format!(
                "could not start '{} mcp'. Install Lightpanda or set OMNIX_LIGHTPANDA",
                binary()
            )
        })?;
        if !status.success() {
            bail!("lightpanda mcp exited with {status}");
        }
        Ok(())
    }
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
}
