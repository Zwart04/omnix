//! Live control between the CLI and the running GUI: a pane registry that the
//! app keeps on disk, an append-only event log, and a loopback socket so the
//! orchestrator can send messages to panes it does not own.
//!
//! The GUI is the only writer of `panes.json`; every command still re-checks
//! the pane against the app's own PTY map. The socket binds 127.0.0.1 with a
//! per-run token stored in a 0600 file, so a process outside the user account
//! cannot reach it.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The laptop's timezone, read once. Shelling out avoids a date library for
/// two strings; formatting itself is plain integer maths.
fn zone() -> &'static (i64, String) {
    static ZONE: OnceLock<(i64, String)> = OnceLock::new();
    ZONE.get_or_init(|| {
        let read = |flag: &str| {
            std::process::Command::new("date")
                .arg(flag)
                .output()
                .ok()
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let offset = read("+%z");
        let seconds = parse_offset(&offset);
        let name = read("+%Z");
        (seconds, if name.is_empty() { offset } else { name })
    })
}

fn parse_offset(offset: &str) -> i64 {
    let bytes = offset.as_bytes();
    if bytes.len() < 5 {
        return 0;
    }
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i64 = offset[1..3].parse().unwrap_or(0);
    let minutes: i64 = offset[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// `2026-11-24 10:30 WIB` in the laptop's zone, never bare UTC.
pub fn local_time(at: u64) -> String {
    let (shift, name) = zone();
    let stamp = at as i64 + shift;
    let days = stamp.div_euclid(86_400);
    let rest = stamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} {name}",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// Howard Hinnant's civil_from_days, valid for the range we care about.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Per-user state home. `OMNIX_HOME` wins; the desktop app normally runs from
/// `$HOME`, so both resolve to the same `.omnix`.
pub fn home() -> PathBuf {
    std::env::var_os("OMNIX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn events_path() -> PathBuf {
    home().join(".omnix").join("events.jsonl")
}

/// One JSON object per line. Appends are a single write under the OS append
/// flag, so the GUI and worker CLIs can share the file without a lock.
pub fn append_event(path: &Path, kind: &str, fields: Value) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut line = match fields {
        Value::Object(map) => Value::Object(map),
        _ => json!({}),
    };
    if let Value::Object(map) = &mut line {
        let at = map.get("at").and_then(Value::as_u64).unwrap_or_else(epoch);
        map.entry("at".to_string()).or_insert(json!(at));
        map.insert("atLocal".into(), json!(local_time(at)));
        map.insert("kind".into(), json!(kind));
    }
    let mut body = serde_json::to_vec(&line)?;
    body.push(b'\n');
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("append event {}", path.display()))?;
    file.write_all(&body)?;
    if path == events_path() {
        crate::plugin::dispatch_hooks(kind, &line);
    }
    Ok(())
}

pub fn event(kind: &str, fields: Value) {
    let _ = append_event(&events_path(), kind, fields);
}

/// Optional and off unless `OMNIX_HOLD` or `~/.omnix/hold.txt` names a
/// substring. Used by `agent send` so a broad message cannot silently run a
/// command the owner wanted to review.
pub fn held(text: &str) -> Option<String> {
    let mut patterns: Vec<String> = Vec::new();
    if let Ok(list) = std::env::var("OMNIX_HOLD") {
        patterns.extend(
            list.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string),
        );
    }
    if let Ok(raw) = std::fs::read_to_string(home().join(".omnix").join("hold.txt")) {
        patterns.extend(
            raw.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_string),
        );
    }
    patterns.into_iter().find(|pattern| text.contains(pattern.as_str()))
}

/// Also invoked only: desktop notifications need `OMNIX_NOTIFY_DESKTOP=1`,
/// webhook needs `OMNIX_NOTIFY_WEBHOOK=<url>`. No timer and no daemon.
pub fn notify(title: &str, body: &str) {
    if std::env::var_os("OMNIX_NOTIFY_DESKTOP").is_some() {
        let _ = std::process::Command::new("notify-send")
            .arg(title)
            .arg(body)
            .spawn();
    }
    if let Ok(url) = std::env::var("OMNIX_NOTIFY_WEBHOOK") {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return;
        }
        let payload = json!({ "title": title, "text": body, "at": epoch() });
        std::thread::spawn(move || {
            let _ = ureq::post(&url)
                .set("content-type", "application/json")
                .timeout(Duration::from_secs(5))
                .send_string(&payload.to_string());
        });
    }
}

/// Runtime files are per user, not per repository, so a CLI started in any
/// folder sees the same panes. `OMNIX_RUN_DIR` exists for tests.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("OMNIX_RUN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".omnix").join("run"))
}

pub fn panes_file(dir: &Path) -> PathBuf {
    dir.join("panes.json")
}

pub fn control_file(dir: &Path) -> PathBuf {
    dir.join("control.json")
}

pub fn write_panes(dir: &Path, panes: &[Value]) -> Result<()> {
    fs::create_dir_all(dir)?;
    let path = panes_file(dir);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&json!({ "panes": panes }))?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn read_panes(dir: &Path) -> Vec<Value> {
    fs::read(panes_file(dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.get("panes").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Machine names shared by the CLI and the GUI. The GUI keeps its short visual
/// names; every exported row uses these.
pub fn machine_state(status: &str, exit_code: Option<i64>) -> &'static str {
    if exit_code.is_some() {
        return "exited";
    }
    match status {
        "working" => "working",
        "approval" => "needs-approval",
        "done" => "turn-done",
        "quota" => "quota",
        "failed" => "exited",
        "idle" => "unknown",
        _ => "unknown",
    }
}

/// Target matching for `agent send|wait`: a pane key is unique, then the
/// handoff name, then the visible label/title. First match wins.
pub fn find_pane<'a>(panes: &'a [Value], target: &str) -> Option<&'a Value> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let field = |pane: &'a Value, key: &str| {
        pane.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
    };
    panes
        .iter()
        .find(|pane| field(pane, "paneKey") == target)
        .or_else(|| {
            panes
                .iter()
                .find(|pane| field(pane, "handoff") == target)
        })
        .or_else(|| {
            panes.iter().find(|pane| {
                field(pane, "label") == target || field(pane, "title") == target
            })
        })
}

#[derive(Debug, Clone)]
pub struct ControlInfo {
    pub port: u16,
    pub token: String,
}

pub fn read_control(dir: &Path) -> Option<ControlInfo> {
    let value: Value = serde_json::from_slice(&fs::read(control_file(dir)).ok()?).ok()?;
    Some(ControlInfo {
        port: value.get("port").and_then(Value::as_u64)? as u16,
        token: value.get("token").and_then(Value::as_str)?.to_string(),
    })
}

/// One request/response per connection, newline-delimited JSON. The handler
/// runs on the GUI event loop, so every answer reflects the live PTY map.
pub struct Server {
    dir: PathBuf,
    token: String,
    pub port: u16,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        // One wake connection, not a poll: the accept loop checks `stop` after
        // each accept and exits, which closes the listener port.
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(info) = read_control(&self.dir) {
            if info.token == self.token {
                let _ = fs::remove_file(control_file(&self.dir));
            }
        }
    }
}

pub fn start<F>(dir: &Path, root: &Path, handler: F) -> Result<Server>
where
    F: Fn(Value) -> Value + Send + Sync + 'static,
{
    fs::create_dir_all(dir)?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind control socket")?;
    let port = listener.local_addr()?.port();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let token = format!(
        "{:x}",
        Sha256::digest(format!("{stamp}:{}:{}", std::process::id(), root.display()).as_bytes())
    );
    let info = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
        "root": root.to_string_lossy(),
        "version": env!("CARGO_PKG_VERSION"),
    });
    write_private(&control_file(dir), serde_json::to_vec_pretty(&info)?.as_slice())?;

    let handler = Arc::new(handler);
    let expected = token.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stop_thread.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = stream else { continue };
            let handler = Arc::clone(&handler);
            let expected = expected.clone();
            std::thread::spawn(move || {
                let _ = serve_one(stream, &expected, &*handler);
            });
        }
    });

    Ok(Server {
        dir: dir.to_path_buf(),
        token,
        port,
        stop,
    })
}

fn serve_one<F>(mut stream: TcpStream, expected: &str, handler: &F) -> Result<()>
where
    F: Fn(Value) -> Value,
{
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.len() > 2 * 1024 * 1024 {
        bail!("control request is too large");
    }
    let request: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
    let token = request.get("token").and_then(Value::as_str).unwrap_or("");
    let reply = if token != expected {
        json!({ "ok": false, "error": "bad control token" })
    } else {
        handler(request)
    };
    let mut body = serde_json::to_vec(&reply)?;
    body.push(b'\n');
    stream.write_all(&body)?;
    stream.flush()?;
    Ok(())
}

/// CLI side. A missing or stale control file becomes one readable error so the
/// orchestrator can tell "app not running" from "pane rejected".
pub fn request(dir: &Path, mut payload: Value) -> Result<Value> {
    let info = read_control(dir).with_context(|| {
        format!(
            "Omnix app is not running (no control file in {})",
            dir.display()
        )
    })?;
    if let Value::Object(map) = &mut payload {
        map.insert("token".into(), json!(info.token));
    }
    let mut stream = TcpStream::connect(("127.0.0.1", info.port))
        .with_context(|| "Omnix app is not reachable; open the app and retry")?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut body = serde_json::to_vec(&payload)?;
    body.push(b'\n');
    stream.write_all(&body)?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let reply: Value = serde_json::from_str(line.trim()).context("read control reply")?;
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        let error = reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("control request failed");
        bail!("{error}");
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-control-{tag}-{}-{}",
            std::process::id(),
            epoch()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn machine_states_match_the_sidebar_marks() {
        assert_eq!(machine_state("working", None), "working");
        assert_eq!(machine_state("approval", None), "needs-approval");
        assert_eq!(machine_state("done", None), "turn-done");
        assert_eq!(machine_state("failed", Some(3)), "exited");
        assert_eq!(machine_state("done", Some(0)), "exited");
        assert_eq!(machine_state("idle", None), "unknown");
    }

    #[test]
    fn panes_roundtrip_and_targets_resolve() {
        let dir = temp_dir("panes");
        let panes = vec![
            json!({"paneKey":"tab:1","handoff":"tugas-a","label":"codex","status":"working"}),
            json!({"paneKey":"tab:2","handoff":"tugas-b","label":"claude","status":"turn-done"}),
        ];
        write_panes(&dir, &panes).unwrap();
        let read = read_panes(&dir);
        let key = |target: &str| {
            find_pane(&read, target)
                .and_then(|pane| pane.get("paneKey"))
                .and_then(Value::as_str)
        };
        assert_eq!(read.len(), 2);
        assert_eq!(key("tab:2"), Some("tab:2"));
        assert_eq!(key("tugas-a"), Some("tab:1"));
        assert_eq!(key("claude"), Some("tab:2"));
        assert_eq!(key("missing"), None);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn events_append_one_json_object_per_line() {
        let dir = temp_dir("events");
        let path = dir.join("events.jsonl");
        append_event(&path, "handoff-created", json!({"name": "a"})).unwrap();
        append_event(&path, "agent-exited", json!({"code": 2})).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let rows: Vec<Value> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["kind"], "handoff-created");
        assert_eq!(rows[1]["code"], 2);
        assert!(rows[0]["at"].as_u64().unwrap() > 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loopback_roundtrip_requires_the_token() {
        let dir = temp_dir("socket");
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let server = start(&dir, &dir, move |req| {
            counter.fetch_add(1, Ordering::SeqCst);
            json!({"ok": true, "echo": req.get("cmd").cloned().unwrap_or(Value::Null)})
        })
        .unwrap();
        let reply = request(&dir, json!({"cmd": "ping"})).unwrap();
        assert_eq!(reply["echo"], "ping");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let port = server.port;
        drop(server);
        assert!(read_control(&dir).is_none(), "drop removes the control file");
        // Shutdown is refuse-only after the wake connection; give the accept
        // thread a bounded moment to close the listener.
        let closed = (0..100).any(|_| {
            if TcpStream::connect(("127.0.0.1", port)).is_err() {
                true
            } else {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        });
        assert!(closed, "listener must close after drop");
        fs::remove_dir_all(&dir).ok();
    }
}
