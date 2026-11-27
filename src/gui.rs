use crate::{browser, history, procs, projects, pty, sources, update, usage, workspace};
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{ResizeDirection, Window, WindowBuilder};
use wry::dpi::{LogicalPosition as WryPos, LogicalSize as WrySize};
use wry::{Rect, WebView, WebViewBuilder};

const INDEX_HTML: &str = include_str!("../app/index.html");
const APP_CSS: &str = include_str!("../app/style.css");
const APP_JS: &str = include_str!("../app/app.js");
const XTERM_JS: &str = include_str!("../app/vendor/xterm.js");
const XTERM_CSS: &str = include_str!("../app/vendor/xterm.css");
const ADDON_FIT_JS: &str = include_str!("../app/vendor/addon-fit.js");
const ADDON_WEBGL_JS: &str = include_str!("../app/vendor/addon-webgl.js");
const TUTORIAL: &str = include_str!("../tutorial/index.html");
const ICON: &str = include_str!("../app/icon.svg");
const AGENT_AMP: &[u8] = include_bytes!("../app/agents/amp.svg");
const AGENT_AIDER: &[u8] = include_bytes!("../app/agents/aider.svg");
const AGENT_CONTINUE: &[u8] = include_bytes!("../app/agents/continue.svg");
const AGENT_DROID: &[u8] = include_bytes!("../app/agents/droid.svg");
const AGENT_FORGE: &[u8] = include_bytes!("../app/agents/forge.svg");
const AGENT_GOOSE: &[u8] = include_bytes!("../app/agents/goose.svg");
const AGENT_JUNIE: &[u8] = include_bytes!("../app/agents/junie.svg");
const AGENT_OPENHANDS: &[u8] = include_bytes!("../app/agents/openhands.svg");
const AGENT_VIBE: &[u8] = include_bytes!("../app/agents/vibe.svg");
const LOGO_PNG: &[u8] = include_bytes!("../app/logo.png");
const LOGO_RGBA: &[u8] = include_bytes!("../app/logo.rgba");
const TAKO_PNG: &[u8] = include_bytes!("../app/tako.png");
const LOGO_SIDE: u32 = 128;

pub enum UserEvent {
    Wake,
}

enum Msg {
    Ui(String),
    Control(Value, Sender<Value>),
    PromptDue(String),
    Probe(String),
    StartTest,
    Finish(bool),
    Reply(u64, Result<Value>),
    DeepCheck,
    PtyData { key: String, data: Vec<u8> },
    PtyExit { key: String },
}

struct PaneProc {
    pid: u32,
    label: String,
    cwd: String,
}

/// The pane registry the CLI sees. The GUI owns it; every write goes through
/// `persist_panes` so a crashed read still finds a complete JSON file.
#[derive(Clone, Default)]
struct PaneMeta {
    label: String,
    title: String,
    cwd: String,
    agent: String,
    handoff: String,
    parent: String,
    session_id: String,
    status: String,
    exit_code: Option<i64>,
    orchestrator: bool,
    updated_at: u64,
}

impl PaneMeta {
    fn json(&self, key: &str) -> Value {
        let status = if self.status.is_empty() {
            "idle"
        } else {
            &self.status
        };
        json!({
            "paneKey": key,
            "label": self.label,
            "title": self.title,
            "cwd": self.cwd,
            "agent": self.agent,
            "handoff": self.handoff,
            "parent": self.parent,
            "sessionId": self.session_id,
            "status": status,
            "state": crate::control::machine_state(status, self.exit_code),
            "exitCode": self.exit_code,
            "orchestrator": self.orchestrator,
            "updatedAt": self.updated_at,
            "updatedAtLocal": crate::control::local_time(self.updated_at),
        })
    }
}

/// Two agents in one folder edit the same files; two workers on one account
/// burn the same quota. Both are advisory: the banner explains the risk and
/// offers a one-click move, but nothing is blocked.
fn pane_conflicts(app: &App) -> Value {
    let mut folders: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut accounts: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (key, meta) in &app.panes {
        if !app.ptys.contains_key(key) {
            continue;
        }
        if meta.agent.trim().is_empty() || meta.status == "exited" {
            continue;
        }
        let row = json!({
            "paneKey": key,
            "agent": meta.agent,
            "title": meta.title,
            "cwd": meta.cwd,
            "sessionId": meta.session_id,
        });
        if !meta.cwd.trim().is_empty() {
            folders
                .entry(meta.cwd.clone())
                .or_default()
                .push(row.clone());
        }
        let account = match crate::agent::split_agent(&meta.agent) {
            Some((base, Some(profile))) => format!("{base}:{profile}"),
            Some((base, None)) => base.to_string(),
            None => meta.agent.clone(),
        };
        accounts.entry(account).or_default().push(row);
    }
    let mut out = Vec::new();
    for (cwd, panes) in folders {
        if panes.len() > 1 {
            out.push(json!({
                "kind": "folder",
                "value": cwd,
                "message": format!("{} agents are running in this folder: {cwd}", panes.len()),
                "panes": panes,
            }));
        }
    }
    for (account, panes) in accounts {
        if panes.len() > 1 {
            out.push(json!({
                "kind": "profile",
                "value": account,
                "message": format!("{} workers use the same account/profile: {account}", panes.len()),
                "panes": panes,
            }));
        }
    }
    Value::Array(out)
}

struct App {
    content: Option<WebView>,
    browser_ctx: Option<wry::WebContext>,
    ui: WebView,
    window: Window,
    root: PathBuf,
    size: (f64, f64),
    content_rect: (f64, f64, f64, f64),
    content_visible: bool,
    ptys: HashMap<String, pty::Pty>,
    pane_procs: HashMap<String, PaneProc>,
    panes: BTreeMap<String, PaneMeta>,
    pane_tails: HashMap<String, String>,
    pane_quota_reported: HashSet<String>,
    pending_prompts: HashMap<String, String>,
    ctrl: Option<crate::control::Server>,
    quit: bool,
}

fn px_rect(w: u32, h: u32) -> Rect {
    Rect {
        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
        size: wry::dpi::PhysicalSize::new(w, h).into(),
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: WryPos::new(x, y).into(),
        size: WrySize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match path {
        "/" | "/index.html" => (INDEX_HTML.as_bytes(), "text/html; charset=utf-8"),
        "/style.css" => (APP_CSS.as_bytes(), "text/css; charset=utf-8"),
        "/app.js" => (APP_JS.as_bytes(), "text/javascript; charset=utf-8"),
        "/vendor/xterm.js" => (XTERM_JS.as_bytes(), "text/javascript; charset=utf-8"),
        "/vendor/xterm.css" => (XTERM_CSS.as_bytes(), "text/css; charset=utf-8"),
        "/vendor/addon-fit.js" => (ADDON_FIT_JS.as_bytes(), "text/javascript; charset=utf-8"),
        "/vendor/addon-webgl.js" => (ADDON_WEBGL_JS.as_bytes(), "text/javascript; charset=utf-8"),
        "/tutorial.html" => (TUTORIAL.as_bytes(), "text/html; charset=utf-8"),
        "/icon.svg" => (ICON.as_bytes(), "image/svg+xml"),
        "/agents/amp.svg" => (AGENT_AMP, "image/svg+xml"),
        "/agents/aider.svg" => (AGENT_AIDER, "image/svg+xml"),
        "/agents/continue.svg" => (AGENT_CONTINUE, "image/svg+xml"),
        "/agents/droid.svg" => (AGENT_DROID, "image/svg+xml"),
        "/agents/forge.svg" => (AGENT_FORGE, "image/svg+xml"),
        "/agents/goose.svg" => (AGENT_GOOSE, "image/svg+xml"),
        "/agents/junie.svg" => (AGENT_JUNIE, "image/svg+xml"),
        "/agents/openhands.svg" => (AGENT_OPENHANDS, "image/svg+xml"),
        "/agents/vibe.svg" => (AGENT_VIBE, "image/svg+xml"),
        "/logo.png" => (LOGO_PNG, "image/png"),
        "/tako.png" => (TAKO_PNG, "image/png"),
        _ => return None,
    })
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn serve_plugin_panel(path: &str) -> Option<wry::http::Response<std::borrow::Cow<'static, [u8]>>> {
    let rest = path.strip_prefix("/plugin-panel/")?;
    let (plugin, rel) = rest.split_once('/')?;
    let plugin = percent_decode(plugin)?;
    let rel = percent_decode(rel)?;
    let (file, kind) = crate::plugin::panel_file(&plugin, &rel).ok()?;
    let body = std::fs::read(file).ok()?;
    Some(
        wry::http::Response::builder()
            .header("Content-Type", kind)
            .header("Cache-Control", "no-store")
            .header(
                "Content-Security-Policy",
                "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'",
            )
            .header("X-Content-Type-Options", "nosniff")
            .body(std::borrow::Cow::Owned(body))
            .ok()?,
    )
}

fn serve(req: wry::http::Request<Vec<u8>>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let path = req.uri().path().to_string();
    if let Some(response) = serve_plugin_panel(&path) {
        return response;
    }
    match asset(&path) {
        Some((body, kind)) => wry::http::Response::builder()
            .header("Content-Type", kind)
            .header("Cache-Control", "no-store")
            .body(std::borrow::Cow::Borrowed(body))
            .unwrap(),
        None => wry::http::Response::builder()
            .status(404)
            .body(std::borrow::Cow::Owned(b"not found".to_vec()))
            .unwrap(),
    }
}

fn js_escape(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn emit(ui: &WebView, event: &str, payload: &Value) {
    let body = payload.to_string();
    let js = format!(
        "window.__omnix&&window.__omnix.emit({},{});",
        js_escape(event),
        js_escape(&body)
    );
    let _ = ui.evaluate_script(&js);
}

fn reply(ui: &WebView, id: u64, result: Result<Value>) {
    let (ok, payload) = match result {
        Ok(value) => (true, value.to_string()),
        Err(err) => (
            false,
            json!({"code": "internal", "message": format!("{err:#}")}).to_string(),
        ),
    };
    let js = format!(
        "window.__omnix&&window.__omnix.reply({id},{ok},{});",
        js_escape(&payload)
    );
    if ui.evaluate_script(&js).is_err() {
        eprintln!("omnix: could not deliver reply {id}");
    }
}

fn reply_later<F>(id: u64, tx: Sender<Msg>, proxy: EventLoopProxy<UserEvent>, job: F)
where
    F: FnOnce() -> Result<Value> + Send + 'static,
{
    std::thread::spawn(move || {
        let result = job();
        let _ = tx.send(Msg::Reply(id, result));
        let _ = proxy.send_event(UserEvent::Wake);
    });
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn url_allowed(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")) && !url.contains(' ')
}

fn history_db(root: &PathBuf) -> PathBuf {
    root.join(".omnix").join("history.sqlite")
}

/// Current branch of a working directory, shown on history cards. A missing
/// repo or a detached HEAD is simply left blank.
fn git_branch(dir: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name)
    }
}

fn as_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn as_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Native folder chooser. Linux asks GTK for a `FileChooserNative`, which the
/// desktop portal renders with the system look; Windows and macOS shell out to
/// their own dialogs. Cancelling is `None`, not an error.
#[cfg(target_os = "linux")]
fn native_pick_folder(app: &App, start: Option<PathBuf>) -> Result<Option<String>> {
    use gtk::prelude::*;
    use tao::platform::unix::WindowExtUnix;

    let dialog = gtk::FileChooserNative::new(
        Some("Add project folder"),
        Some(app.window.gtk_window()),
        gtk::FileChooserAction::SelectFolder,
        Some("Add project"),
        Some("Cancel"),
    );
    if let Some(dir) = start {
        dialog.set_current_folder(dir);
    }
    let accepted = dialog.run() == gtk::ResponseType::Accept;
    let picked = dialog.file().and_then(|file| file.path());
    dialog.destroy();
    Ok(if accepted {
        picked.map(|path| path.to_string_lossy().into_owned())
    } else {
        None
    })
}

#[cfg(target_os = "windows")]
fn native_pick_folder(_app: &App, start: Option<PathBuf>) -> Result<Option<String>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut script = String::from(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
         $d.Description = 'Add project folder';",
    );
    if let Some(dir) = start {
        let dir = dir.to_string_lossy().replace('\'', "''");
        script.push_str(&format!(" $d.SelectedPath = '{dir}';"));
    }
    script.push_str(
        " if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) \
         { Write-Output $d.SelectedPath }",
    );

    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("open the Windows folder picker")?;
    if !out.status.success() {
        return Ok(None);
    }
    let picked = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if picked.is_empty() {
        None
    } else {
        Some(picked)
    })
}

#[cfg(target_os = "macos")]
fn native_pick_folder(_app: &App, start: Option<PathBuf>) -> Result<Option<String>> {
    let mut script =
        String::from("POSIX path of (choose folder with prompt \"Add project folder\"");
    if let Some(dir) = start {
        let dir = dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        script.push_str(&format!(" default location POSIX file \"{dir}\""));
    }
    script.push(')');

    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("open the macOS folder picker")?;
    if !out.status.success() {
        return Ok(None);
    }
    let picked = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_end_matches('/')
        .to_string();
    Ok(if picked.is_empty() {
        None
    } else {
        Some(picked)
    })
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn native_pick_folder(_app: &App, _start: Option<PathBuf>) -> Result<Option<String>> {
    Ok(None)
}

fn persist_panes(app: &App) {
    let rows: Vec<Value> = app
        .panes
        .iter()
        .map(|(key, meta)| meta.json(key))
        .collect();
    let _ = crate::control::write_panes(&crate::control::runtime_dir(), &rows);
}

fn find_pane_key(app: &App, target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    app.panes
        .iter()
        .find(|(key, _)| key.as_str() == target)
        .or_else(|| {
            app.panes
                .iter()
                .find(|(_, meta)| meta.handoff == target)
        })
        .or_else(|| {
            app.panes.iter().find(|(_, meta)| {
                meta.label == target || meta.title == target
            })
        })
        .map(|(key, _)| key.clone())
}

fn log_state(app: &App, key: &str, meta: &PaneMeta, old: &str, new: &str) {
    if old == new {
        return;
    }
    let kind = match new {
        "done" => "agent-turn-done",
        "approval" => "agent-needs-approval",
        "failed" => "agent-exited",
        "working" => "agent-working",
        _ => return,
    };
    let pane = app.ptys.contains_key(key);
    crate::control::event(
        kind,
        json!({
            "pane": key,
            "handoff": meta.handoff,
            "agent": meta.agent,
            "title": meta.title,
            "previous": old,
            "live": pane,
        }),
    );
    if new == "done" || new == "approval" {
        let who = if meta.handoff.is_empty() { &meta.title } else { &meta.handoff };
        crate::control::notify(
            if new == "done" { "Worker finished" } else { "Worker needs approval" },
            who,
        );
    }
}

fn ai_memory(app: &App) -> Value {
    let table = procs::snapshot();
    let mut rss = 0u64;
    let mut count = 0usize;
    for pane in app.pane_procs.values() {
        let (pane_rss, pane_count) = table.sum_tree(pane.pid);
        if pane_count > 0 {
            rss = rss.saturating_add(pane_rss);
            count += pane_count;
        }
    }
    let limit = std::env::var("OMNIX_AI_RAM_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|mb| *mb > 0)
        .map(|mb| mb.saturating_mul(1024 * 1024))
        .or_else(usage::default_budget);
    json!({
        "rss": rss,
        "processes": count,
        "limitBytes": limit,
        "over": limit.map(|limit| rss > limit).unwrap_or(false),
    })
}

fn handle(app: &mut App, raw: &str, tx: &Sender<Msg>, proxy: &EventLoopProxy<UserEvent>) {
    let parsed: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(e) => {
            emit(
                &app.ui,
                "toast",
                &json!({"level": "error", "message": format!("bad request: {e}")}),
            );
            return;
        }
    };
    let id = parsed.get("id").and_then(Value::as_u64).unwrap_or(0);
    let cmd = parsed.get("cmd").and_then(Value::as_str).unwrap_or("");
    let args = parsed.get("args").cloned().unwrap_or_else(|| json!({}));

    let result = match cmd {
        "appInfo" => Ok({
            let saved = crate::agent::load_settings(&app.root);
            let engine_version = browser::version();
            let mut agents = crate::agent::AGENTS
                .iter()
                .map(|a| json!({
                    "name": a,
                    "bin": crate::agent::launch_cmd(a),
                    "available": crate::agent::available(a),
                    "enabled": saved.get(*a).map(|c| c.enabled).unwrap_or(true),
                }))
                .collect::<Vec<_>>();
            agents.extend(crate::agent::profile_rows(&app.root));
            agents.extend(crate::plugin::agent_rows());
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "root": app.root.to_string_lossy(),
                "engine": {
                    "lightpanda": engine_version.is_some(),
                    "version": engine_version,
                },
                "platform": std::env::consts::OS,
                "traceKeys": app.root.join(".trace-keys").exists(),
                "agents": agents,
                "pluginCommands": crate::plugin::command_names(),
                "pluginPanels": crate::plugin::panel_list(),
                "devBuilds": crate::dev::builds().len(),
                "setup": crate::setup::payload(&app.root),
            })
        }),
        "setupComplete" => (|| {
            crate::setup::complete(&app.root)?;
            Ok(json!({ "ok": true, "setup": crate::setup::payload(&app.root) }))
        })(),
        "setupReset" => (|| {
            crate::setup::reset(&app.root)?;
            Ok(json!({ "ok": true, "setup": crate::setup::payload(&app.root) }))
        })(),
        "supportOpen" => (|| {
            crate::setup::open_support()?;
            Ok(json!({ "ok": true, "url": crate::setup::SUPPORT_URL }))
        })(),
        "windowMinimize" => {
            app.window.set_minimized(true);
            Ok(json!({ "ok": true }))
        }
        "windowMaximize" => {
            let maximized = !app.window.is_maximized();
            app.window.set_maximized(maximized);
            Ok(json!({ "maximized": maximized }))
        }
        "windowClose" => {
            app.quit = true;
            Ok(json!({ "ok": true }))
        }
        "windowDrag" => (|| {
            app.window
                .drag_window()
                .map_err(|err| anyhow!("drag window: {err}"))?;
            Ok(json!({ "ok": true }))
        })(),
        "windowResize" => (|| {
            let dir = as_str(&args, "dir").unwrap_or_default();
            let dir = match dir.as_str() {
                "n" => ResizeDirection::North,
                "s" => ResizeDirection::South,
                "e" => ResizeDirection::East,
                "w" => ResizeDirection::West,
                "ne" => ResizeDirection::NorthEast,
                "nw" => ResizeDirection::NorthWest,
                "se" => ResizeDirection::SouthEast,
                "sw" => ResizeDirection::SouthWest,
                _ => return Err(anyhow!("unknown resize direction")),
            };
            app.window
                .drag_resize_window(dir)
                .map_err(|err| anyhow!("resize window: {err}"))?;
            Ok(json!({ "ok": true }))
        })(),
        "updateCheck" => {
            let tx = tx.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                let result = update::check_payload();
                let _ = tx.send(Msg::Reply(id, result));
                let _ = proxy.send_event(UserEvent::Wake);
            });
            return;
        }
        "updateInstall" => {
            let tx = tx.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                let result = update::install(true);
                let _ = tx.send(Msg::Reply(id, result));
                let _ = proxy.send_event(UserEvent::Wake);
            });
            return;
        }
        "projects" => Ok(json!({
            "projects": projects::load(&app.root),
            "path": projects::store_path(&app.root).to_string_lossy(),
        })),
        "projectAdd" => (|| {
            let path = as_str(&args, "path").ok_or_else(|| anyhow!("path is required"))?;
            if path.len() > 4096 {
                return Err(anyhow!("project path is too long"));
            }
            Ok(json!({ "projects": projects::add(&app.root, &path)? }))
        })(),
        "projectRemove" => (|| {
            let path = as_str(&args, "path").ok_or_else(|| anyhow!("path is required"))?;
            Ok(json!({ "projects": projects::remove(&app.root, &path)? }))
        })(),
        "pickFolder" => (|| {
            // Native folder chooser: GTK portal on Linux, the system dialog on
            // Windows and macOS. The UI never draws its own file browser.
            let start = as_str(&args, "path")
                .map(PathBuf::from)
                .filter(|path| path.is_dir());
            Ok(json!({ "path": native_pick_folder(app, start)? }))
        })(),
        "usage" => Ok(crate::usage::snapshot_live_for(
            &app.root,
            &crate::control::home(),
        )),
        "agentSettings" => (|| {
            let saved = crate::agent::load_settings(&app.root);
            let defaults = crate::agent::load_defaults(&app.root);
            Ok(json!({
                "path": crate::agent::settings_path(&app.root).to_string_lossy(),
                "defaults": { "fullAccess": defaults.full_access },
                "profiles": crate::agent::profile_rows(&app.root),
                "agents": crate::agent::AGENTS
                    .iter()
                    .map(|name| {
                        let cfg = saved.get(*name);
                        json!({
                            "name": name,
                            "bin": crate::agent::launch_cmd(name),
                            "available": crate::agent::available(name),
                            "enabled": cfg.map(|c| c.enabled).unwrap_or(true),
                            "args": cfg.map(|c| c.args.clone()).unwrap_or_default(),
                            "env": cfg.map(|c| c.env.clone()).unwrap_or_default(),
                            "fullAccessFlag": crate::agent::full_access_flag(name),
                        })
                    })
                    .chain(crate::plugin::agent_rows())
                    .collect::<Vec<_>>(),
            }))
        })(),
        "agentSettingsSave" => (|| {
            let name = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if crate::agent::split_agent(&name).is_none() {
                return Err(anyhow!("unknown agent '{name}'"));
            }
            let extra_args = as_str(&args, "args").unwrap_or_default();
            let env = as_str(&args, "env").unwrap_or_default();
            if extra_args.len() > 400 || env.len() > 400 {
                return Err(anyhow!("agent settings are too long"));
            }
            let is_profile = crate::agent::profile_of(&name).is_some();
            let label = as_str(&args, "label").unwrap_or_default();
            let config_dir = as_str(&args, "configDir").unwrap_or_default();
            if label.len() > 80 || config_dir.len() > 500 {
                return Err(anyhow!("profile settings are too long"));
            }
            if label.chars().any(char::is_control) || config_dir.chars().any(char::is_control) {
                return Err(anyhow!("profile settings contain control characters"));
            }
            let mut saved = crate::agent::load_settings(&app.root);
            let mut cfg = saved.remove(&name).unwrap_or_default();
            cfg.args = extra_args;
            cfg.env = env;
            if is_profile {
                cfg.label = label.trim().to_string();
                cfg.config_dir = config_dir.trim().to_string();
                if let Some(share) = args.get("shareProjects").and_then(Value::as_bool) {
                    cfg.share_projects = share;
                }
            }
            let custom = is_profile
                || !cfg.args.trim().is_empty()
                || !cfg.env.trim().is_empty()
                || !cfg.label.trim().is_empty()
                || !cfg.config_dir.trim().is_empty()
                || cfg.share_projects
                || !cfg.enabled;
            if custom {
                saved.insert(name.clone(), cfg);
            }
            crate::agent::save_settings(&app.root, &saved)?;
            Ok(json!({ "ok": true }))
        })(),
        "agentProfileDelete" => (|| {
            let name = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if crate::agent::profile_of(&name).is_none() {
                return Err(anyhow!("'{name}' is not a profile"));
            }
            let mut saved = crate::agent::load_settings(&app.root);
            saved.remove(&name);
            crate::agent::save_settings(&app.root, &saved)?;
            Ok(json!({ "ok": true }))
        })(),
        "agentEnable" => (|| {
            let name = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if crate::agent::split_agent(&name).is_none() {
                return Err(anyhow!("unknown agent '{name}'"));
            }
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("enabled must be a boolean"))?;
            let mut saved = crate::agent::load_settings(&app.root);
            let mut cfg = saved.remove(&name).unwrap_or_default();
            cfg.enabled = enabled;
            if crate::agent::profile_of(&name).is_none()
                && cfg.args.trim().is_empty()
                && cfg.env.trim().is_empty()
                && enabled
            {
                // enabled is the default; keep the store empty
            } else {
                saved.insert(name.clone(), cfg);
            }
            crate::agent::save_settings(&app.root, &saved)?;
            Ok(json!({ "ok": true, "enabled": enabled }))
        })(),
        "plugins" => Ok(json!({
            "plugins": crate::plugin::list(),
            "agents": crate::plugin::agent_rows(),
            "catalog": std::env::var("OMNIX_PLUGIN_CATALOG").unwrap_or_else(|_| "public GitHub index".into()),
            "path": crate::plugin::plugins_dir().to_string_lossy(),
        })),
        "pluginAction" => {
            let action = as_str(&args, "action").unwrap_or_default();
            let name = as_str(&args, "name").unwrap_or_default();
            let names = args
                .get("names")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            if matches!(action.as_str(), "install" | "update" | "search" | "doctor") {
                let tx = tx.clone();
                let proxy = proxy.clone();
                reply_later(id, tx, proxy, move || match action.as_str() {
                    "install" => crate::plugin::install(&name),
                    "update" => crate::plugin::update(&names),
                    "search" => crate::plugin::search(&name),
                    _ => Ok(crate::plugin::doctor()),
                });
                return;
            }
            (|| match action.as_str() {
                "remove" => crate::plugin::remove(&name),
                "enable" => crate::plugin::set_enabled(&name, true),
                "disable" => crate::plugin::set_enabled(&name, false),
                "revoke" => crate::plugin::revoke(&name, &names),
                "grant" => crate::plugin::grant(&name, &names),
                "list" => Ok(json!({ "plugins": crate::plugin::list() })),
                _ => Err(anyhow!("unknown plugin action '{action}'")),
            })()
        }
        "pluginRun" => {
            let name = match as_str(&args, "name") {
                Some(name) => name,
                None => {
                    reply(&app.ui, id, Err(anyhow!("plugin name is required")));
                    return;
                }
            };
            let command = as_str(&args, "command").unwrap_or_default();
            let forwarded = args
                .get("args")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            let tx = tx.clone();
            let proxy = proxy.clone();
            reply_later(id, tx, proxy, move || {
                crate::plugin::invoke(
                    &name,
                    "command",
                    json!({"plugin": name, "command": command, "args": forwarded}),
                )
            });
            return;
        }
        "pluginPanel" => (|| {
            let name = as_str(&args, "name").ok_or_else(|| anyhow!("plugin name is required"))?;
            crate::plugin::panel_info(&name, as_str(&args, "panel").as_deref())
        })(),
        "pluginPanelCall" => {
            let name = match as_str(&args, "name") {
                Some(name) => name,
                None => {
                    reply(&app.ui, id, Err(anyhow!("plugin name is required")));
                    return;
                }
            };
            let method = as_str(&args, "method").unwrap_or_else(|| "panel".into());
            let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
            let tx = tx.clone();
            let proxy = proxy.clone();
            reply_later(id, tx, proxy, move || crate::plugin::invoke(&name, &method, params));
            return;
        }
        "devStatus" => Ok(crate::dev::status()),
        "devTaskInfo" => (|| {
            let name = as_str(&args, "name").ok_or_else(|| anyhow!("task name is required"))?;
            let record = crate::handoff::list_all()
                .into_iter()
                .find(|record| record.name == name)
                .ok_or_else(|| anyhow!("task '{name}' not found"))?;
            let brief = std::fs::read_to_string(&record.brief).unwrap_or_default();
            Ok(json!({ "record": record, "brief": brief }))
        })(),
        "devTaskDelivered" => (|| {
            let name = as_str(&args, "name").ok_or_else(|| anyhow!("task name is required"))?;
            let pane = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            let mut record = crate::handoff::list_all()
                .into_iter()
                .find(|record| record.name == name)
                .ok_or_else(|| anyhow!("task '{name}' not found"))?;
            crate::handoff::mark_delivered(&mut record, &pane)?;
            Ok(json!({ "ok": true, "name": name, "paneKey": pane }))
        })(),
        "devTask" => (|| {
            let brief = as_str(&args, "brief").ok_or_else(|| anyhow!("brief is required"))?;
            if brief.len() > 512 * 1024 {
                return Err(anyhow!("brief is too large"));
            }
            let agent = as_str(&args, "agent").unwrap_or_else(|| "codex".into());
            let name = as_str(&args, "name");
            let dir = crate::dev::dev_home().join("briefs");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}.md", crate::control::epoch()));
            std::fs::write(&path, brief)?;
            let (record, receipt) = crate::dev::task(crate::dev::TaskOptions {
                prompt_file: path,
                agent,
                name,
                repo: None,
                run: false,
            })?;
            let brief = std::fs::read_to_string(&record.brief).unwrap_or_default();
            Ok(json!({
                "record": record,
                "path": receipt.to_string_lossy(),
                "brief": brief,
            }))
        })(),
        "devBuild" => {
            let tx = tx.clone();
            let proxy = proxy.clone();
            let workspace = as_str(&args, "workspace").map(PathBuf::from);
            reply_later(id, tx, proxy, move || {
                let build = crate::dev::build(workspace, true)?;
                Ok(serde_json::to_value(build)?)
            });
            return;
        }
        "devTest" => {
            let tx = tx.clone();
            let proxy = proxy.clone();
            let workspace = as_str(&args, "workspace").map(PathBuf::from);
            reply_later(id, tx, proxy, move || crate::dev::test(workspace, true));
            return;
        }
        "devRun" => (|| {
            let build = as_str(&args, "build");
            crate::dev::run(build.as_deref())
        })(),
        "devInstall" => (|| {
            let build = as_str(&args, "build");
            crate::dev::install(build.as_deref(), false)
        })(),
        "devRestore" => (|| crate::dev::restore(false))(),
        "agentDefaultsSave" => (|| {
            let full_access = args
                .get("fullAccess")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("fullAccess must be a boolean"))?;
            crate::agent::save_defaults(&app.root, &crate::agent::AgentDefaults { full_access })?;
            Ok(json!({ "ok": true, "defaults": { "fullAccess": full_access } }))
        })(),
        "uiTrace" => {
            // Opt-in keyboard diagnostics: only writes when <root>/.trace-keys exists.
            let root = app.root.clone();
            let line = args
                .get("line")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if root.join(".trace-keys").exists() {
                use std::io::Write as _;
                let path = root.join("keys.log");
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(f, "{line}");
                }
            }
            Ok(json!({ "ok": true }))
        }
        "status" => (|| {
            let db = history_db(&app.root);
            let conn = history::open(&db)?;
            let (sessions, messages, bytes) = history::status(&conn, &db)?;
            let workspaces = workspace::list(&app.root)?;
            Ok(json!({
                "root": app.root.to_string_lossy(),
                "sessions": sessions,
                "messages": messages,
                "dbBytes": bytes,
                "workspaces": workspaces,
                "conflicts": pane_conflicts(app),
            }))
        })(),
        "historyIndex" => (|| {
            let dir = as_str(&args, "dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.root.join(".omnix").join("transcripts"));
            let agent = as_str(&args, "agent");
            let db = history_db(&app.root);
            let mut conn = history::open(&db)?;
            let report = history::index_dir(
                &mut conn,
                &dir,
                agent.as_deref(),
                &app.root.to_string_lossy(),
            )?;
            Ok(json!({
                "files": report.files,
                "messages": report.messages,
                "skippedLines": report.skipped_lines,
                "db": db.to_string_lossy(),
            }))
        })(),
        "historyScan" => {
            // First scan can walk hundreds of files: keep the event loop free.
            let root = app.root.clone();
            let tx_scan = tx.clone();
            let proxy_scan = proxy.clone();
            let probe_on = std::env::var_os("OMNIX_GUI_PROBE").is_some();
            std::thread::spawn(move || {
                if probe_on {
                    eprintln!("scan thread start");
                }
                let start = std::time::Instant::now();
                let result = (|| -> Result<Value> {
                    let db = history_db(&root);
                    if probe_on {
                        eprintln!("scan: opening {}", db.display());
                    }
                    let mut conn = history::open(&db)?;
                    if probe_on {
                        eprintln!("scan: opened, walking sources");
                    }
                    let report = sources::scan(&mut conn, &root)?;
                    if probe_on {
                        eprintln!("scan: walked {} files", report.files);
                    }
                    let counts = history::agent_counts(&conn)?;
                    Ok(json!({
                        "files": report.files,
                        "messages": report.messages,
                        "skipped": report.skipped,
                        "unchanged": report.unchanged,
                        "pruned": report.pruned,
                        "db": db.to_string_lossy(),
                        "agents": counts
                            .iter()
                            .map(|c| json!({"agent": c.agent, "sessions": c.sessions, "messages": c.messages}))
                            .collect::<Vec<_>>(),
                        "roots": sources::roots(&root)
                            .iter()
                            .map(|(agent, path)| json!({"agent": agent, "path": path.to_string_lossy()}))
                            .collect::<Vec<_>>(),
                    }))
                })();
                println!(
                    "scan thread done in {}ms ok={}",
                    start.elapsed().as_millis(),
                    result.is_ok()
                );
                let _ = tx_scan.send(Msg::Reply(id, result));
                let _ = proxy_scan.send_event(UserEvent::Wake);
            });
            return;
        }
        "historyProjects" => (|| {
            let conn = history::open(&history_db(&app.root))?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(60) as usize;
            let rows = history::projects(&conn, limit)?;
            let projects: Vec<Value> = rows
                .into_iter()
                .map(|p| {
                    let name = std::path::Path::new(&p.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.path.clone());
                    json!({
                        "path": p.path,
                        "name": name,
                        "sessions": p.sessions,
                        "messages": p.messages,
                        "updatedAt": p.updated_at,
                        "agents": p.agents.split(',').filter(|a| !a.is_empty()).collect::<Vec<_>>(),
                    })
                })
                .collect();
            Ok(json!({ "root": app.root.to_string_lossy(), "projects": projects }))
        })(),
        "historySessions" => (|| {
            let conn = history::open(&history_db(&app.root))?;
            let scope = as_str(&args, "scope").unwrap_or_else(|| "all".into());
            let project = as_str(&args, "cwd").filter(|cwd| !cwd.is_empty());
            let cwd_prefix = match scope.as_str() {
                "workspace" => Some(app.root.to_string_lossy().to_string()),
                "project" => Some(
                    project
                        .clone()
                        .unwrap_or_else(|| app.root.to_string_lossy().to_string()),
                ),
                _ => None,
            };
            let agent = as_str(&args, "agent");
            let query = as_str(&args, "query");
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(80) as usize;
            let rows = history::list(
                &conn,
                agent.as_deref(),
                cwd_prefix.as_deref(),
                query.as_deref(),
                limit,
            )?;
            let total = history::count(
                &conn,
                agent.as_deref(),
                cwd_prefix.as_deref(),
                query.as_deref(),
            )?;
            let mut branches: HashMap<String, Option<String>> = HashMap::new();
            let sessions: Vec<Value> = rows
                .into_iter()
                .map(|r| {
                    let branch = r.cwd.as_deref().and_then(|cwd| {
                        branches
                            .entry(cwd.to_string())
                            .or_insert_with(|| git_branch(cwd))
                            .clone()
                    });
                    json!({
                        "agent": r.agent,
                        "sessionId": r.session_id,
                        "title": r.title,
                        "cwd": r.cwd,
                        "updatedAt": r.updated_at,
                        "messageCount": r.message_count,
                        "resumeCommand": r.resume_command,
                        "logPath": r.file_path,
                        "model": r.model,
                        "branch": branch,
                        "preview": r.preview,
                    })
                })
                .collect();
            Ok(json!({
                "scope": scope,
                "root": app.root.to_string_lossy(),
                "total": total,
                "sessions": sessions,
            }))
        })(),
        "historyDeleteSession" => (|| {
            let agent = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            let session = as_str(&args, "sessionId")
                .ok_or_else(|| anyhow!("sessionId is required"))?;
            let conn = history::open(&history_db(&app.root))?;
            let removed = history::remove_session(&conn, &agent, &session)?;
            Ok(json!({ "removed": removed }))
        })(),
        "openPath" => (|| {
            let path = as_str(&args, "path").ok_or_else(|| anyhow!("path is required"))?;
            let mut target = PathBuf::from(&path);
            if as_bool(&args, "folder") {
                if let Some(parent) = target.parent() {
                    target = parent.to_path_buf();
                }
            }
            if !target.exists() {
                return Err(anyhow!("path does not exist: {}", target.display()));
            }
            crate::setup::open_path(&target)?;
            Ok(json!({ "path": target.to_string_lossy() }))
        })(),
        "resumeWorktree" => (|| {
            let cwd = as_str(&args, "cwd").ok_or_else(|| anyhow!("cwd is required"))?;
            let agent = as_str(&args, "agent").unwrap_or_else(|| "agent".into());
            let session = as_str(&args, "sessionId").unwrap_or_default();
            let title = as_str(&args, "title").unwrap_or_else(|| "session".into());
            let key = format!("hist:{agent}:{session}");
            let moved = crate::handoff::worktree_for_pane(
                std::path::Path::new(&cwd),
                &key,
                &format!("{agent} {title}"),
            )?;
            Ok(json!({
                "name": moved.name,
                "worktree": moved.worktree.to_string_lossy(),
                "branch": moved.branch,
            }))
        })(),
        "historySearch" => (|| {
            let query = as_str(&args, "query").ok_or_else(|| anyhow!("query is required"))?;
            let conn = history::open(&history_db(&app.root))?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            let hits = history::search(
                &conn,
                &query,
                as_str(&args, "agent").as_deref(),
                as_bool(&args, "conversation"),
                as_bool(&args, "newest"),
                limit,
            )?;
            let hits: Vec<Value> = hits
                .into_iter()
                .map(|h| {
                    json!({
                        "agent": h.agent,
                        "sessionId": h.session_id,
                        "title": h.title,
                        "cwd": h.cwd,
                        "updatedAt": h.updated_at,
                        "score": h.score,
                        "snippet": h.snippet,
                        "resumeCommand": h.resume_command,
                    })
                })
                .collect();
            Ok(json!({ "hits": hits }))
        })(),
        "ptySpawn" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            if key.len() > 64
                || !key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
            {
                return Err(anyhow!("invalid paneKey"));
            }
            let cwd = as_str(&args, "cwd")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.root.clone());
            let shell = as_str(&args, "shell");
            let agent = as_str(&args, "agent");
            let session = as_str(&args, "sessionId");
            let command = as_str(&args, "command");
            let label = as_str(&args, "label")
                .or_else(|| agent.clone())
                .or_else(|| {
                    command
                        .as_deref()
                        .and_then(|line| line.split_whitespace().next())
                        .map(str::to_string)
                })
                .or_else(|| shell.clone())
                .unwrap_or_else(|| "terminal".into());
            let label: String = label.chars().filter(|c| !c.is_control()).take(48).collect();
            let options = crate::agent::LaunchOptions {
                model: as_str(&args, "model"),
                effort: as_str(&args, "effort"),
                mode: as_str(&args, "mode"),
                prompt: as_str(&args, "prompt"),
            };
            let extra = match &agent {
                Some(agent) => crate::agent::extra_args(agent, &options)?,
                None => Vec::new(),
            };
            let mut extra = extra;
            let generated_session = match (&agent, &session) {
                (Some(a), None) if crate::agent::base_agent(a) == "claude" => {
                    let sid = crate::agent::new_session_id();
                    extra.extend(crate::agent::new_session_args(a, &sid));
                    Some(sid)
                }
                _ => None,
            };
            let extra_env = match &agent {
                Some(agent) => crate::agent::model_env(agent, options.model.as_deref()),
                None => Vec::new(),
            };
            let login = args.get("login").and_then(Value::as_bool).unwrap_or(false);
            let (spawned, initial_prompt) = if login {
                let agent = agent
                    .as_deref()
                    .ok_or_else(|| anyhow!("agent is required for a login tab"))?;
                let (spawned, prompt) = pty::spawn_login(&app.root, agent, &cwd)?;
                (spawned, prompt)
            } else {
                let spawned = match (agent.clone(), session.clone(), command) {
                    (_, _, Some(line)) => {
                        if line.len() > 240 {
                            return Err(anyhow!("command is too long"));
                        }
                        pty::spawn_command(&line, &cwd)?
                    }
                    (Some(a), s, None) => {
                        pty::spawn_agent_with(&app.root, &a, s.as_deref(), &cwd, &extra, &extra_env)?
                    }
                    _ => pty::spawn(shell.as_deref(), &cwd, None)?,
                };
                (spawned, None)
            };
            let pid = spawned.pid;
            let mut reader = spawned.reader;
            let tx_data = tx.clone();
            let proxy_data = proxy.clone();
            let key_thread = key.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx_data
                                .send(Msg::PtyData {
                                    key: key_thread.clone(),
                                    data: buf[..n].to_vec(),
                                })
                                .is_err()
                            {
                                break;
                            }
                            let _ = proxy_data.send_event(UserEvent::Wake);
                        }
                    }
                }
                let _ = tx_data.send(Msg::PtyExit { key: key_thread });
                let _ = proxy_data.send_event(UserEvent::Wake);
            });
            app.ptys.insert(key.clone(), spawned.pty);
            app.pane_procs.insert(
                key.clone(),
                PaneProc {
                    pid,
                    label: label.clone(),
                    cwd: cwd.to_string_lossy().into_owned(),
                },
            );
            // Auto track: a resumed session is already named, so sleep/resume
            // can find it later without another CLI call.
            if let (Some(agent), Some(session)) = (agent.as_deref(), session.as_deref()) {
                let _ = workspace::track(&app.root, &cwd, agent, session, &cwd, &key);
            }
            if let (Some(agent), Some(sid)) = (agent.as_deref(), generated_session.as_deref()) {
                let _ = workspace::track(&app.root, &cwd, agent, sid, &cwd, &key);
            }
            let handoff = as_str(&args, "handoff").unwrap_or_default();
            let title = as_str(&args, "title").unwrap_or_else(|| label.clone());
            let meta = PaneMeta {
                label: label.clone(),
                title: title.chars().filter(|c| !c.is_control()).take(80).collect(),
                cwd: cwd.to_string_lossy().into_owned(),
                agent: as_str(&args, "agent").unwrap_or_default(),
                handoff: handoff.clone(),
                parent: as_str(&args, "parent").unwrap_or_default(),
                session_id: generated_session
                    .clone()
                    .or_else(|| as_str(&args, "sessionId"))
                    .unwrap_or_default(),
                status: "working".into(),
                exit_code: None,
                orchestrator: as_bool(&args, "orchestrator"),
                updated_at: crate::control::epoch(),
            };
            if !handoff.is_empty() {
                crate::control::event(
                    "agent-started",
                    json!({ "pane": key.clone(), "handoff": handoff, "agent": meta.agent.clone() }),
                );
            }
            app.panes.insert(key.clone(), meta);
            if let Some(text) = initial_prompt {
                app.pending_prompts.insert(key.clone(), text);
                let timer_key = key.clone();
                let timer_tx = tx.clone();
                let timer_proxy = proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(2500));
                    let _ = timer_tx.send(Msg::PromptDue(timer_key));
                    let _ = timer_proxy.send_event(UserEvent::Wake);
                });
            }
            persist_panes(app);
            Ok(json!({ "paneKey": key, "pid": pid }))
        })(),
        "ptyWrite" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            let data = args.get("data").and_then(Value::as_str).unwrap_or("");
            let session = app
                .ptys
                .get_mut(&key)
                .ok_or_else(|| anyhow!("unknown pty"))?;
            use std::io::Write;
            session.writer.write_all(data.as_bytes())?;
            session.writer.flush()?;
            Ok(json!({ "ok": true }))
        })(),
        "ptyResize" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            let cols = args
                .get("cols")
                .and_then(Value::as_u64)
                .unwrap_or(80)
                .clamp(1, 500) as u16;
            let rows = args
                .get("rows")
                .and_then(Value::as_u64)
                .unwrap_or(24)
                .clamp(1, 300) as u16;
            let session = app
                .ptys
                .get_mut(&key)
                .ok_or_else(|| anyhow!("unknown pty"))?;
            session.master.resize(portable_pty::PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            Ok(json!({ "ok": true }))
        })(),
        "ptyKill" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            if let Some(mut session) = app.ptys.remove(&key) {
                let _ = session.child.kill();
            }
            app.pane_procs.remove(&key);
            app.panes.remove(&key);
            app.pane_tails.remove(&key);
            app.pane_quota_reported.remove(&key);
            persist_panes(app);
            Ok(json!({ "ok": true }))
        })(),
        "paneState" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            let (old, new) = {
                let meta = app.panes.entry(key.clone()).or_default();
                let old = meta.status.clone();
                if let Some(value) = as_str(&args, "status") {
                    meta.status = value;
                }
                if let Some(value) = as_str(&args, "handoff") {
                    meta.handoff = value;
                }
                if let Some(value) = as_str(&args, "parent") {
                    meta.parent = value;
                }
                if let Some(value) = as_str(&args, "title") {
                    meta.title = value.chars().filter(|c| !c.is_control()).take(80).collect();
                }
                if let Some(value) = as_str(&args, "agent") {
                    meta.agent = value;
                }
                if let Some(value) = as_str(&args, "sessionId") {
                    meta.session_id = value;
                }
                if let Some(value) = as_str(&args, "cwd") {
                    meta.cwd = value;
                }
                if let Some(value) = args.get("orchestrator").and_then(Value::as_bool) {
                    meta.orchestrator = value;
                }
                if let Some(value) = args.get("exitCode").and_then(Value::as_i64) {
                    meta.exit_code = Some(value);
                }
                meta.updated_at = crate::control::epoch();
                (old, meta.status.clone())
            };
            if let Some(meta) = app.panes.get(&key) {
                log_state(app, &key, meta, &old, &new);
            }
            let exit = app.panes.get(&key).and_then(|meta| meta.exit_code);
            persist_panes(app);
            Ok(json!({ "ok": true, "paneKey": key, "state": crate::control::machine_state(&new, exit) }))
        })(),
        "paneMoveWorktree" => (|| {
            let key = as_str(&args, "paneKey").ok_or_else(|| anyhow!("paneKey is required"))?;
            let meta = app
                .panes
                .get(&key)
                .cloned()
                .ok_or_else(|| anyhow!("unknown pane '{key}'"))?;
            if meta.agent.trim().is_empty() {
                return Err(anyhow!("only agent panes can move to a worktree"));
            }
            if !app.ptys.contains_key(&key) {
                return Err(anyhow!("pane '{key}' is not running"));
            }
            let cwd = PathBuf::from(&meta.cwd);
            let repo = crate::handoff::main_repo(&cwd)?;
            let moved = crate::handoff::worktree_for_pane(&repo, &key, &meta.label)?;
            Ok(json!({
                "paneKey": key,
                "repo": repo.to_string_lossy(),
                "name": moved.name,
                "branch": moved.branch,
                "worktree": moved.worktree.to_string_lossy(),
                "agent": meta.agent,
                "sessionId": meta.session_id,
                "label": meta.label,
                "title": meta.title,
                "handoff": meta.handoff,
                "parent": meta.parent,
                "orchestrator": meta.orchestrator,
            }))
        })(),
        "agentSend" => (|| {
            let reply = control_request(
                app,
                &json!({
                    "cmd": "send",
                    "target": as_str(&args, "target").unwrap_or_default(),
                    "text": as_str(&args, "text").unwrap_or_default(),
                    "force": as_bool(&args, "force"),
                }),
                tx,
                proxy,
            );
            if reply.get("ok").and_then(Value::as_bool) == Some(false) {
                let message = reply
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("send failed");
                return Err(anyhow!("{message}"));
            }
            Ok(reply)
        })(),
        "handoffList" => Ok(json!({ "handoffs": crate::handoff::list_all() })),
        "handoffClose" => (|| {
            let name = as_str(&args, "name").ok_or_else(|| anyhow!("name is required"))?;
            let record = crate::handoff::close(&name)?;
            Ok(json!({ "name": record.name, "worktree": record.worktree }))
        })(),
        "gc" => {
            let dry = args.get("dryRun").and_then(Value::as_bool).unwrap_or(true);
            crate::gc::run(&app.root, dry)
        }
        "memory" => (|| {
            let table = procs::snapshot();
            let pid = std::process::id();
            let (app_rss, app_count) = table.sum_tree(pid);
            let mut rows = Vec::new();
            let mut ai_rss = 0u64;
            let mut ai_count = 0usize;
            for (key, pane) in &app.pane_procs {
                let (rss, count) = table.sum_tree(pane.pid);
                let top = table.top_names(pane.pid, 3);
                if count == 0 {
                    continue;
                }
                ai_rss = ai_rss.saturating_add(rss);
                ai_count += count;
                rows.push((
                    rss,
                    json!({
                        "key": key,
                        "label": pane.label,
                        "cwd": pane.cwd,
                        "pid": pane.pid,
                        "rss": rss,
                        "processes": count,
                        "top": top,
                    }),
                ));
            }
            rows.sort_by(|a, b| b.0.cmp(&a.0));
            let panes: Vec<Value> = rows.into_iter().map(|(_, row)| row).collect();
            let limit = std::env::var("OMNIX_AI_RAM_MB")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|mb| *mb > 0)
                .map(|mb| mb.saturating_mul(1024 * 1024))
                .or_else(usage::default_budget);
            Ok(json!({
                "available": !table.is_empty(),
                "pid": pid,
                "app": { "rss": app_rss, "processes": app_count },
                "ai": { "rss": ai_rss, "processes": ai_count },
                "other": {
                    "rss": app_rss.saturating_sub(ai_rss),
                    "processes": app_count.saturating_sub(ai_count),
                },
                "budget": {
                    "limitBytes": limit,
                    "over": limit.map(|limit| ai_rss > limit).unwrap_or(false),
                },
                "panes": panes,
                "conflicts": pane_conflicts(app),
                "ram": usage::ram(),
            }))
        })(),
        "browserOpen" => (|| {
            let url = as_str(&args, "url").ok_or_else(|| anyhow!("url is required"))?;
            if !url_allowed(&url) {
                return Ok(
                    json!({"ok": false, "code": "blocked_scheme", "message": "only http and https"}),
                );
            }
            let (x, y, w, h) = read_rect(&args);
            app.content_rect = (x, y, w, h);
            app.content_visible = true;
            match app.content.as_mut() {
                Some(view) => {
                    view.set_bounds(rect(x, y, w, h))?;
                    view.set_visible(true)?;
                    view.load_url(&url)?;
                }
                None => {
                    let data_dir = browser::native_data_dir();
                    let _ = std::fs::create_dir_all(&data_dir);
                    let context = app
                        .browser_ctx
                        .get_or_insert_with(|| wry::WebContext::new(Some(data_dir)));
                    let view = WebViewBuilder::new_with_web_context(context)
                        .with_bounds(rect(x, y, w, h))
                        .with_url(&url)
                        .with_devtools(false)
                        .with_navigation_handler(|target| url_allowed(&target))
                        .build_as_child(&app.window)?;
                    app.content = Some(view);
                }
            }
            Ok(json!({ "url": url }))
        })(),
        "browserBounds" => (|| {
            let (x, y, w, h) = read_rect(&args);
            app.content_rect = (x, y, w, h);
            app.content_visible = args.get("visible").and_then(Value::as_bool).unwrap_or(true);
            if let Some(view) = app.content.as_ref() {
                view.set_bounds(rect(x, y, w, h))?;
                view.set_visible(app.content_visible)?;
            }
            Ok(json!({ "ok": true }))
        })(),
        "browserClose" => (|| {
            app.content = None;
            app.content_visible = false;
            Ok(json!({ "ok": true }))
        })(),
        "browserStop" => (|| {
            let value = browser::stop(None, false)?;
            Ok(value)
        })(),
        "browserFetch" => (|| {
            let url = as_str(&args, "url").ok_or_else(|| anyhow!("url is required"))?;
            if !url_allowed(&url) {
                return Ok(
                    json!({"ok": false, "code": "blocked_scheme", "message": "only http and https"}),
                );
            }
            let dump = as_str(&args, "dump").unwrap_or_else(|| "markdown".into());
            let text = browser::fetch_text(&url, &dump)?;
            Ok(json!({ "text": text }))
        })(),
        "handoffAgent" => (|| {
            let agent = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if crate::agent::launch_cmd(&agent).is_none() {
                return Err(anyhow!("unknown agent '{agent}'"));
            }
            let source_agent = as_str(&args, "sourceAgent").unwrap_or_default();
            let session = as_str(&args, "sessionId").unwrap_or_default();
            let focused = as_str(&args, "context").as_deref() != Some("full");
            let cwd = as_str(&args, "cwd")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.root.clone());
            let mut model = as_str(&args, "model").unwrap_or_default();
            if let Some(root) = crate::team::find_root(&cwd) {
                let team = crate::team::load_at(&root)?;
                if let Some(member) = crate::team::resolve(&team, &agent, &model)? {
                    model = member.model.clone();
                }
            }

            let (body, title, messages, source_cwd) =
                if source_agent.is_empty() || session.is_empty() {
                    (
                        String::from("_(no source session selected)_"),
                        String::new(),
                        0,
                        None,
                    )
                } else {
                    let conn = history::open(&history_db(&app.root))?;
                    match history::transcript(&conn, &source_agent, &session, focused) {
                        Ok(t) => (t.text, t.title, t.messages, t.cwd),
                        Err(err) => (
                            format!("_(transcript unavailable: {err:#})_"),
                            String::new(),
                            0,
                            None,
                        ),
                    }
                };

            let dir = cwd.join(".omnix").join("handoffs");
            std::fs::create_dir_all(&dir)?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let brief = dir.join(format!("{stamp}-to-{agent}.md"));
            let text = format!(
                "# Handoff to {agent}\n\n- From: {source_agent} {session}\n- Cwd: {}\n- Context: {}\n- Messages: {messages}\n- Created: {stamp}\n\n## Stopping point\n\n{body}",
                cwd.display(),
                if focused { "focused" } else { "full" },
            );
            std::fs::write(&brief, &text)?;
            Ok(json!({
                "agent": agent,
                "cwd": cwd.to_string_lossy(),
                "brief": brief.to_string_lossy(),
                "title": title,
                "messages": messages,
                "sourceCwd": source_cwd,
                "model": model,
                "preview": text.chars().take(1400).collect::<String>(),
                "prompt": text,
            }))
        })(),
        "fallbackShow" => (|| {
            let agent = as_str(&args, "agent").unwrap_or_default();
            Ok(json!({
                "configured": crate::fallback::get(&app.root, &agent),
                "candidates": crate::fallback::candidates(&app.root, &agent),
            }))
        })(),
        "fallbackSet" => (|| {
            let agent = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            let target = as_str(&args, "target").unwrap_or_default();
            let command = as_str(&args, "command");
            if target.is_empty() && command.is_none() {
                return Err(anyhow!("pick a fallback agent or type a command"));
            }
            let entry = crate::fallback::Fallback {
                agent: target,
                command,
                auto: args.get("auto").and_then(Value::as_bool).unwrap_or(false),
                enabled: true,
            };
            crate::fallback::set(&app.root, &agent, entry.clone())?;
            Ok(json!({ "ok": true, "fallback": entry }))
        })(),
        "fallbackRemove" => (|| {
            let agent = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            crate::fallback::remove(&app.root, &agent)?;
            Ok(json!({ "ok": true }))
        })(),
        "teamGet" => (|| {
            let project = as_str(&args, "project").ok_or_else(|| anyhow!("project is required"))?;
            let root = crate::team::find_root(Path::new(&project))
                .unwrap_or_else(|| PathBuf::from(&project));
            let team = crate::team::load_at(&root)?;
            Ok(json!({
                "project": root.to_string_lossy(),
                "team": team,
            }))
        })(),
        "teamSave" => (|| {
            let project = as_str(&args, "project").ok_or_else(|| anyhow!("project is required"))?;
            let root = crate::team::find_root(Path::new(&project))
                .unwrap_or_else(|| PathBuf::from(&project));
            let raw = args
                .get("team")
                .cloned()
                .ok_or_else(|| anyhow!("team is required"))?;
            let team: crate::team::Team = serde_json::from_value(raw)?;
            let saved = crate::team::save_at(&root, team)?;
            Ok(json!({
                "project": root.to_string_lossy(),
                "team": saved,
            }))
        })(),
        "teamStart" => (|| {
            let project = as_str(&args, "project").ok_or_else(|| anyhow!("project is required"))?;
            let root = crate::team::find_root(Path::new(&project))
                .unwrap_or_else(|| PathBuf::from(&project));
            let task = as_str(&args, "task").unwrap_or_default();
            let task = task.trim();
            if task.is_empty() {
                return Err(anyhow!("describe the team task first"));
            }
            if task.chars().count() > 4000 {
                return Err(anyhow!("team task is too long (4000 characters max)"));
            }
            let team = crate::team::load_at(&root)?;
            if team.mode != "auto" {
                return Err(anyhow!("start is only enabled for auto teams"));
            }
            if team.members.len() < 2 {
                return Err(anyhow!("an auto team needs an orchestrator and at least one worker"));
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let mut names = Vec::new();
            for (idx, member) in team.members.iter().enumerate() {
                let base: String = member
                    .agent
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let name = format!("team-{stamp}-{}-{base}", idx + 1);
                if name.len() > 48 {
                    return Err(anyhow!("generated team name is too long"));
                }
                names.push(name);
            }
            let orch_index = team
                .members
                .iter()
                .position(|m| crate::team::member_key(m) == team.orchestrator)
                .ok_or_else(|| anyhow!("orchestrator is not a member"))?;
            let orch_name = names[orch_index].clone();
            let mut workers: Vec<Value> = Vec::new();
            let mut worker_lines = String::new();
            for (idx, member) in team.members.iter().enumerate() {
                if idx == orch_index {
                    continue;
                }
                let name = &names[idx];
                let model = if member.model.is_empty() {
                    None
                } else {
                    Some(member.model.clone())
                };
                let role = crate::team::role_note(&member.role, "implementer");
                let prompt = format!(
                    "# Team worker: {name}\n\nProject: {}\nTask: {task}\nRole: {role}\n\nYou report to the orchestrator `{orch_name}`. Wait for a message from the orchestrator before editing. Reply `READY` and stop; do not start on your own. When the assignment arrives, work only in this worktree, run the tests, and summarize the result.\n",
                    root.display()
                );
                let (record, _worktree) = crate::handoff::create_with(
                    &app.root,
                    crate::handoff::CreateOptions {
                        name: name.clone(),
                        agent: member.agent.clone(),
                        prompt: prompt.clone(),
                        repo: Some(root.clone()),
                        parent: Some(orch_name.clone()),
                        model: model.clone(),
                        mode: Some("edit".into()),
                        on_quota: crate::team::quota_spec(&team, member),
                        ..Default::default()
                    },
                )?;
                worker_lines.push_str(&format!(
                    "- `{}` — {} — {} — worktree `{}`\n",
                    record.name,
                    member.agent,
                    model.as_deref().unwrap_or("agent default"),
                    record.worktree
                ));
                workers.push(json!({
                    "name": record.name,
                    "agent": member.agent,
                    "model": model.unwrap_or_default(),
                    "cwd": record.worktree,
                    "branch": record.branch,
                    "brief": record.brief,
                    "prompt": prompt,
                    "orchestrator": false,
                    "parent": record.parent,
                }));
            }
            let orch_member = &team.members[orch_index];
            let orch_role = crate::team::role_note(&orch_member.role, "planner");
            let orchestrator_prompt = format!(
                "# Team orchestrator: {orch_name}\n\nProject: {}\nTask: {task}\nRole: {orch_role}\nQuota policy: {}\n\nWorkers:\n{worker_lines}\nStart by checking `omnix agent status`; wait a few seconds until every worker is listed. Split the task into one assignment per worker and send it with:\n\n  omnix agent send <worker> --text \"...\"\n  omnix agent wait <worker> --until turn-done,needs-approval,exited,quota\n\nIf `agent wait` exits with code 75 (quota exhausted), Omnix saves a WIP commit and sends you a notice automatically; check `omnix events --kind agent-quota-exhausted` and reassign remaining work.\n\nUse the worktree path shown above when you refer to a worker's checkout. Keep the main checkout untouched. When all workers report back, summarize the result and the tests that passed.\n",
                root.display(),
                team.on_quota
            );
            let orch_model = if orch_member.model.is_empty() {
                None
            } else {
                Some(orch_member.model.clone())
            };
            let (record, _worktree) = crate::handoff::create_with(
                &app.root,
                crate::handoff::CreateOptions {
                    name: orch_name.clone(),
                    agent: orch_member.agent.clone(),
                    prompt: orchestrator_prompt.clone(),
                    repo: Some(root.clone()),
                    model: orch_model.clone(),
                    mode: Some("edit".into()),
                    on_quota: crate::team::quota_spec(&team, orch_member),
                    orchestrator: true,
                    ..Default::default()
                },
            )?;
            let mut out = workers;
            out.push(json!({
                "name": record.name,
                "agent": orch_member.agent,
                "model": orch_model.unwrap_or_default(),
                "cwd": record.worktree,
                "branch": record.branch,
                "brief": record.brief,
                "prompt": orchestrator_prompt,
                "orchestrator": true,
                "parent": record.parent,
            }));
            Ok(json!({ "project": root.to_string_lossy(), "members": out }))
        })(),
        "selftest" => {
            if std::env::var_os("OMNIX_SELFTEST").is_none() {
                return reply(&app.ui, id, Err(anyhow!("selftest is disabled")));
            }
            let step = as_str(&args, "step").unwrap_or_else(|| "?".into());
            let ok = as_bool(&args, "ok");
            let detail = args.get("detail").cloned().unwrap_or(Value::Null);
            println!(
                "selftest {step}: {} {}",
                if ok { "ok" } else { "FAIL" },
                detail_text(&detail)
            );
            if step == "done" {
                let _ = tx.send(Msg::Finish(ok));
                let _ = proxy.send_event(UserEvent::Wake);
            }
            Ok(json!({ "ok": true }))
        }
        "" => Err(anyhow!("missing cmd")),
        other => Err(anyhow!("unknown cmd '{other}'")),
    };

    reply(&app.ui, id, result);
}

/// End-to-end UI check: clicks a real session card, opens a browser tab, then
/// reports what the DOM actually did. Only runs with `OMNIX_GUI_PROBE=deep`.
const DEEP_JS: &str = r#"(function(){
  var out = {steps: [], ok: false};
  function say(name, pass, info) { out.steps.push({name: name, pass: !!pass, info: info == null ? null : String(info).slice(0, 120)}); }
  function waitFor(test, limit) {
    return new Promise(function (resolve) {
      var started = Date.now();
      (function poll() {
        var value = null;
        try { value = test(); } catch (err) { value = null; }
        if (value) return resolve(value);
        if (Date.now() - started > limit) return resolve(null);
        setTimeout(poll, 150);
      })();
    });
  }
  var cards = function () { return document.querySelectorAll('#history-list > .sess'); };
  (async function () {
    var list = await waitFor(function () { return cards().length > 0 ? cards() : null; }, 30000);
    say('history list', !!list, list ? list.length + ' cards' : 'no cards');
    if (!list) { out.ok = false; window.__omnixDeep = out; return; }

    var before = document.querySelectorAll('#tabstrip-tabs > *').length;
    var resume = list[0].querySelector('.sess-resume') || list[0].querySelector('.sess-main');
    if (resume) resume.click();
    var termPane = await waitFor(function () {
      return document.querySelectorAll('#tabstrip-tabs > *').length > before &&
        document.querySelector('.pane-term') ? document.querySelector('.pane-term') : null;
    }, 15000);
    say('resume opens terminal tab', !!termPane,
      document.querySelectorAll('#tabstrip-tabs > *').length + ' tabs');

    var xterm = await waitFor(function () { return document.querySelector('.pane-term .term-host .xterm') ? 1 : null; }, 8000);
    say('xterm mounted', !!xterm, xterm ? 'xterm' : 'no xterm');
    var rows = await waitFor(function () {
      var node = document.querySelector('.pane-term .xterm-rows');
      return node && node.textContent.trim().length ? node : null;
    }, 10000);
    say('pty output rendered', !!rows, rows ? rows.textContent.trim().slice(0, 60) : 'no rows');

    var tabsBefore = document.querySelectorAll('#tabstrip-tabs > *').length;
    var add = document.getElementById('tab-new');
    if (add) add.click();
    var menu = await waitFor(function () { return document.getElementById('tab-new-browser') ? 1 : null; }, 3000);
    if (menu) document.getElementById('tab-new-browser').click();
    var browserPane = await waitFor(function () { return document.querySelector('.pane-browser') ? 1 : null; }, 8000);
    say('browser tab opens', !!browserPane, document.querySelectorAll('#tabstrip-tabs > *').length + ' tabs');

    var urlBar = document.getElementById('browser-url');
    say('browser controls present', !!urlBar, urlBar ? urlBar.placeholder : 'missing');

    var rail = document.querySelectorAll('.panel-btn[data-panel]');
    var handoffBtn = null;
    for (var i = 0; i < rail.length; i++) { if (rail[i].dataset.panel === 'handoff') handoffBtn = rail[i]; }
    if (handoffBtn) handoffBtn.click();
    var form = await waitFor(function () { return document.getElementById('handoff-form') ? 1 : null; }, 4000);
    say('handoff panel opens', !!form, form ? 'form' : 'missing');

    var palette = document.getElementById('palette');
    document.dispatchEvent(new KeyboardEvent('keydown', {key: 'k', ctrlKey: true, bubbles: true}));
    var paletteOpen = await waitFor(function () { return palette && !palette.hidden ? 1 : null; }, 3000);
    say('palette opens', !!paletteOpen, paletteOpen ? 'open' : 'closed');
    if (paletteOpen) document.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true}));

    var fail = 0;
    for (var j = 0; j < out.steps.length; j++) if (!out.steps[j].pass) fail++;
    out.ok = fail === 0;
    out.failed = fail;
    window.__omnixDeep = out;
  })();
})()"#;

/// Debug aid: proves the embedded UI loaded, its JS parsed, and that the
/// indexed-history panels actually filled in.
fn probe(ui: &WebView, url: &str) {
    if std::env::var_os("OMNIX_GUI_PROBE").is_none() {
        return;
    }
    let js = "(function(){try{var d=document.documentElement,q=function(s){return document.querySelectorAll(s).length};\
        var h=document.getElementById('history-list');return JSON.stringify({title:document.title,\
        nodes:q('*'),bridge:typeof window.__omnix,\
        overflowX:d.scrollWidth>d.clientWidth,overflowY:d.scrollHeight>d.clientHeight,\
        vw:d.clientWidth,vh:d.clientHeight,\
        accent:getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),\
        rail:(document.getElementById('rail')||{}).offsetWidth||0,\
        side:(document.getElementById('sidebar')||{}).offsetWidth||0,\
        right:(document.getElementById('agents-panel')||{}).offsetWidth||0,\
        tabstrip:(document.getElementById('tabstrip')||{}).offsetHeight||0,\
        tabs:q('#tabstrip-tabs > *'),cards:q('#history-list > *'),\
        projects:q('#workspace-list > *'),agents:q('#history-agents > *'),\
        historyCount:((document.getElementById('history-count')||{}).textContent||'').trim(),\
        historyMeta:((document.getElementById('history-meta')||{}).textContent||'').trim(),\
        historyState:h?(h.className||''):'missing',\
        deep:window.__omnixDeep?JSON.stringify(window.__omnixDeep):'pending',\
        firstCard:((document.querySelector('#history-list > *')||{}).textContent||'').trim().slice(0,90),\
        visible:Array.from(document.querySelectorAll('button')).filter(function(b){return b.offsetParent}).length});}catch(e){return JSON.stringify({error:String(e)})}})()";
    let url = url.to_string();
    let _ = ui.evaluate_script_with_callback(js, move |result| {
        println!("probe {url}: {result}");
    });
}

/// The first paint is empty on purpose: sample again once the scan lands.
fn resample(tx: &Sender<Msg>, proxy: &EventLoopProxy<UserEvent>) {
    if std::env::var_os("OMNIX_GUI_PROBE").is_none() || SAMPLED.swap(true, Ordering::Relaxed) {
        return;
    }
    let deep = std::env::var("OMNIX_GUI_PROBE")
        .map(|v| v == "deep")
        .unwrap_or(false);
    let (tx, proxy) = (tx.clone(), proxy.clone());
    let _ = std::thread::spawn(move || {
        for (index, wait) in [5u64, 20, 40].into_iter().enumerate() {
            std::thread::sleep(std::time::Duration::from_secs(wait));
            if index == 0 && deep {
                let _ = tx.send(Msg::DeepCheck);
            }
            let _ = tx.send(Msg::Probe("omnix://localhost/".into()));
            let _ = proxy.send_event(UserEvent::Wake);
        }
    });
}

static SAMPLED: AtomicBool = AtomicBool::new(false);

fn start_selftest(ui: &WebView, root: &std::path::Path) {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(240));
        eprintln!("selftest: TIMEOUT");
        std::process::exit(1);
    });
    crate::selftest::seed(root);
    let _ = ui.evaluate_script(crate::selftest::JS);
}

fn detail_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn read_rect(args: &Value) -> (f64, f64, f64, f64) {
    let rect = args.get("rect").cloned().unwrap_or_else(|| json!({}));
    let get = |key: &str| rect.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    (get("x"), get("y"), get("width"), get("height"))
}

fn shutdown(app: &mut App) {
    for (_, mut session) in app.ptys.drain() {
        let _ = session.child.kill();
    }
    app.panes.clear();
    persist_panes(app);
    app.content = None;
    app.browser_ctx = None;
    let _ = crate::preview::stop_all(&app.root, false);
}

fn control_reply(result: Result<Value>) -> Value {
    match result {
        Ok(mut value) => {
            if let Value::Object(map) = &mut value {
                map.entry("ok".to_string()).or_insert(json!(true));
            }
            value
        }
        Err(err) => json!({ "ok": false, "error": format!("{err:#}") }),
    }
}

fn control_request(
    app: &mut App,
    req: &Value,
    tx: &Sender<Msg>,
    proxy: &EventLoopProxy<UserEvent>,
) -> Value {
    let cmd = req.get("cmd").and_then(Value::as_str).unwrap_or("");
    let result: Result<Value> = (|| match cmd {
        "ping" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "root": app.root.to_string_lossy(),
            "pid": std::process::id(),
            "panes": app.panes.len(),
        })),
        "panes" | "state" => Ok(json!({
            "panes": app.panes.iter().map(|(key, meta)| meta.json(key)).collect::<Vec<_>>(),
            "memory": ai_memory(app),
        })),
        "handoffList" => Ok(json!({ "handoffs": crate::handoff::list_all() })),
        "openUrl" => {
            let url = req.get("url").and_then(Value::as_str).unwrap_or("");
            if !url_allowed(url) {
                return Err(anyhow!("only http and https URLs are allowed"));
            }
            emit(&app.ui, "browserNav", &json!({ "url": url, "open": true }));
            Ok(json!({ "url": url }))
        }
        "send" => {
            let target = req.get("target").and_then(Value::as_str).unwrap_or("");
            let text = req.get("text").and_then(Value::as_str).unwrap_or("");
            let force = req.get("force").and_then(Value::as_bool).unwrap_or(false);
            if text.is_empty() || text.len() > 1024 * 1024 {
                return Err(anyhow!("message is empty or too large"));
            }
            if !force {
                if let Some(pattern) = crate::control::held(text) {
                    return Err(anyhow!(
                        "message contains a held command ('{pattern}'); pass --force"
                    ));
                }
            }
            let key = find_pane_key(app, target)
                .ok_or_else(|| anyhow!("no Omnix pane matches '{target}'"))?;
            let state = app
                .panes
                .get(&key)
                .map(|meta| meta.status.clone())
                .unwrap_or_default();
            if state == "approval" && !force {
                return Err(anyhow!(
                    "pane is waiting for approval; answer it in the app or pass --force"
                ));
            }
            let session = app
                .ptys
                .get_mut(&key)
                .ok_or_else(|| anyhow!("pane '{key}' has no live process"))?;
            use std::io::Write;
            let paste = if text.contains('\n') {
                format!("\x1b[200~{}\x1b[201~\r", text.trim_end())
            } else {
                format!("{text}\r")
            };
            session.writer.write_all(paste.as_bytes())?;
            session.writer.flush()?;
            let handoff = app
                .panes
                .get(&key)
                .map(|meta| meta.handoff.clone())
                .unwrap_or_default();
            crate::control::event(
                "agent-message-sent",
                json!({ "pane": key.clone(), "handoff": handoff, "bytes": text.len() }),
            );
            Ok(json!({ "paneKey": key, "state": crate::control::machine_state(&state, None) }))
        }
        "spawnHandoff" => {
            let record = req.get("record").cloned().unwrap_or(Value::Null);
            let name = record
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("handoff record has no name"))?;
            let agent = record
                .get("agent")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("handoff record has no agent"))?;
            let worktree = record
                .get("worktree")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("handoff record has no worktree"))?;
            let cwd = PathBuf::from(worktree);
            if !cwd.is_dir() {
                return Err(anyhow!("worktree is missing: {}", cwd.display()));
            }
            let key = format!("ho:{name}");
            if app.ptys.contains_key(&key) {
                return Err(anyhow!("handoff pane already open: {key}"));
            }
            let options = crate::agent::LaunchOptions {
                model: record.get("model").and_then(Value::as_str).map(str::to_string),
                effort: record.get("effort").and_then(Value::as_str).map(str::to_string),
                mode: matches!(crate::agent::base_agent(agent), "antigravity" | "claude")
                    .then(|| record.get("mode").and_then(Value::as_str).map(str::to_string))
                    .flatten(),
                prompt: None,
            };
            let mut extra = crate::agent::extra_args(agent, &options)?;
            let generated_session = if crate::agent::base_agent(agent) == "claude" {
                let sid = crate::agent::new_session_id();
                extra.extend(crate::agent::new_session_args(agent, &sid));
                Some(sid)
            } else {
                None
            };
            if let Some(sid) = &generated_session {
                if let Some(mut rec) = crate::handoff::find_any(name) {
                    rec.session_id = Some(sid.clone());
                    let _ = crate::handoff::save(&rec);
                }
            }
            let extra_env = crate::agent::model_env(agent, options.model.as_deref());
            let spawned = pty::spawn_agent_with(&app.root, agent, None, &cwd, &extra, &extra_env)?;
            let pid = spawned.pid;
            let mut reader = spawned.reader;
            let tx_data = tx.clone();
            let proxy_data = proxy.clone();
            let key_thread = key.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx_data
                                .send(Msg::PtyData {
                                    key: key_thread.clone(),
                                    data: buf[..n].to_vec(),
                                })
                                .is_err()
                            {
                                break;
                            }
                            let _ = proxy_data.send_event(UserEvent::Wake);
                        }
                    }
                }
                let _ = tx_data.send(Msg::PtyExit { key: key_thread });
                let _ = proxy_data.send_event(UserEvent::Wake);
            });
            app.ptys.insert(key.clone(), spawned.pty);
            app.pane_procs.insert(
                key.clone(),
                PaneProc {
                    pid,
                    label: name.to_string(),
                    cwd: worktree.to_string(),
                },
            );
            let parent = record
                .get("parent")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let meta = PaneMeta {
                label: name.to_string(),
                title: name.to_string(),
                cwd: worktree.to_string(),
                agent: agent.to_string(),
                handoff: name.to_string(),
                parent,
                session_id: generated_session.clone().unwrap_or_default(),
                status: "working".into(),
                exit_code: None,
                orchestrator: record
                    .get("orchestrator")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                updated_at: crate::control::epoch(),
            };
            app.panes.insert(key.clone(), meta.clone());
            if let Some(brief) = record.get("brief").and_then(Value::as_str) {
                if let Ok(text) = std::fs::read_to_string(brief) {
                    if text.len() <= 256 * 1024 {
                        app.pending_prompts.insert(key.clone(), text);
                    }
                }
            }
            let timer_key = key.clone();
            let timer_tx = tx.clone();
            let timer_proxy = proxy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(3500));
                let _ = timer_tx.send(Msg::PromptDue(timer_key));
                let _ = timer_proxy.send_event(UserEvent::Wake);
            });
            emit(&app.ui, "paneOpen", &meta.json(&key));
            crate::control::event(
                "agent-started",
                json!({ "pane": key.clone(), "handoff": name, "agent": agent, "root": "control" }),
            );
            persist_panes(app);
            Ok(json!({ "paneKey": key, "pid": pid }))
        }
        "closePane" => {
            let target = req.get("target").and_then(Value::as_str).unwrap_or("");
            let key = find_pane_key(app, target)
                .ok_or_else(|| anyhow!("no Omnix pane matches '{target}'"))?;
            if let Some(mut session) = app.ptys.remove(&key) {
                let _ = session.child.kill();
            }
            app.pane_procs.remove(&key);
            app.panes.remove(&key);
            app.pane_tails.remove(&key);
            app.pane_quota_reported.remove(&key);
            persist_panes(app);
            emit(&app.ui, "paneClosed", &json!({ "paneKey": key.clone() }));
            Ok(json!({ "paneKey": key }))
        }
        "gc" => {
            let dry = req.get("dryRun").and_then(Value::as_bool).unwrap_or(true);
            Ok(crate::gc::run(&app.root, dry)?)
        }
        _ => Err(anyhow!("unknown control command '{cmd}'")),
    })();
    control_reply(result)
}

fn drain(app: &mut App, rx: &Receiver<Msg>, tx: &Sender<Msg>, proxy: &EventLoopProxy<UserEvent>) {
    let mut chunks: HashMap<String, Vec<u8>> = HashMap::new();
    let mut exited: Vec<String> = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Msg::Ui(raw) => handle(app, &raw, tx, proxy),
            Msg::Control(request, reply_tx) => {
                let response = control_request(app, &request, tx, proxy);
                let _ = reply_tx.send(response);
            }
            Msg::PromptDue(key) => {
                if let Some(text) = app.pending_prompts.remove(&key) {
                    if let Some(session) = app.ptys.get_mut(&key) {
                        use std::io::Write;
                        let paste = format!("\x1b[200~{}\x1b[201~\r", text.trim_end());
                        if session.writer.write_all(paste.as_bytes()).is_ok() {
                            let _ = session.writer.flush();
                            let name = app
                                .panes
                                .get(&key)
                                .map(|meta| meta.handoff.clone())
                                .unwrap_or_default();
                            crate::control::event(
                                "handoff-prompt-sent",
                                json!({ "pane": key, "handoff": name }),
                            );
                            emit(
                                &app.ui,
                                "toast",
                                &json!({"level": "ok", "message": "Handoff prompt sent"}),
                            );
                        }
                    }
                }
            }
            Msg::PtyData { key, data } => {
                if !app.pane_quota_reported.contains(&key) {
                    let is_agent = app
                        .panes
                        .get(&key)
                        .map(|m| !m.agent.trim().is_empty() && !m.orchestrator)
                        .unwrap_or(false);
                    if is_agent {
                        let text_data = String::from_utf8_lossy(&data).into_owned();
                        let needs_id = app
                            .panes
                            .get(&key)
                            .map(|m| m.session_id.is_empty() && !m.agent.trim().is_empty())
                            .unwrap_or(false);
                        if needs_id {
                            if let Some(sid) = crate::agent::extract_session_id(&text_data) {
                                if let Some(meta) = app.panes.get_mut(&key) {
                                    meta.session_id = sid;
                                }
                                persist_panes(app);
                            }
                        }
                        let tail = app.pane_tails.entry(key.clone()).or_default();
                        tail.push_str(&text_data);
                        if tail.len() > 8192 {
                            let drop_to = tail.len() - 4096;
                            if let Some(idx) = tail
                                .char_indices()
                                .find(|(i, _)| *i >= drop_to)
                                .map(|(i, _)| i)
                            {
                                tail.drain(..idx);
                            }
                        }
                        if let Some(ex) = crate::usage::detect_exhaustion(tail) {
                            app.pane_quota_reported.insert(key.clone());
                            let excerpt: String = tail
                                .lines()
                                .rev()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("")
                                .trim()
                                .chars()
                                .take(300)
                                .collect();
                            let (handoff_name, agent_name, parent_name, cwd_name, label_name) =
                                match app.panes.get_mut(&key) {
                                    Some(meta) => {
                                        meta.status = "quota".into();
                                        meta.updated_at = crate::control::epoch();
                                        (
                                            meta.handoff.clone(),
                                            meta.agent.clone(),
                                            meta.parent.clone(),
                                            meta.cwd.clone(),
                                            meta.label.clone(),
                                        )
                                    }
                                    None => Default::default(),
                                };
                            let mut wip_sha: Option<String> = None;
                            if !handoff_name.is_empty() {
                                if let Some(mut rec) = crate::handoff::find_any(&handoff_name) {
                                    let rec_model = rec.model.clone();
                                    let rec_push_wip = rec.push_wip;
                                    if let Ok(wip) = crate::handoff::save_wip_and_report(
                                        &mut rec,
                                        &agent_name,
                                        rec_model.as_deref(),
                                        Some(&key),
                                        &excerpt,
                                        &ex,
                                        rec_push_wip,
                                    ) {
                                        wip_sha = wip.wip_sha;
                                    }
                                }
                            } else {
                                let dirty = if !cwd_name.is_empty() {
                                    crate::handoff::git_dirty_files(Path::new(&cwd_name)).len()
                                } else {
                                    0
                                };
                                crate::control::event(
                                    "agent-quota-exhausted",
                                    json!({
                                        "agent": agent_name,
                                        "model": Value::Null,
                                        "handoff": Value::Null,
                                        "pane": key,
                                        "resetAt": ex.reset_at,
                                        "resetLocal": ex.reset_local,
                                        "retryable": ex.retryable,
                                        "confidence": ex.confidence,
                                        "pesanAsli": excerpt,
                                        "perubahanBelumCommit": dirty,
                                    }),
                                );
                            }
                            persist_panes(app);
                            emit(
                                &app.ui,
                                "toast",
                                &json!({
                                    "level": "warn",
                                    "message": format!(
                                        "Quota exhausted: {} ({})",
                                        if handoff_name.is_empty() { &label_name } else { &handoff_name },
                                        agent_name
                                    )
                                }),
                            );
                            let orch_key = app
                                .panes
                                .iter()
                                .find(|(k, m)| {
                                    *k != &key
                                        && app.ptys.contains_key(*k)
                                        && ((!parent_name.is_empty()
                                            && (*k == &parent_name || m.handoff == parent_name))
                                            || m.orchestrator)
                                })
                                .map(|(k, _)| k.clone());
                            if let Some(ok_key) = orch_key {
                                if let Some(orch_pty) = app.ptys.get_mut(&ok_key) {
                                    use std::io::Write;
                                    let who = if handoff_name.is_empty() {
                                        label_name.as_str()
                                    } else {
                                        handoff_name.as_str()
                                    };
                                    let notice = format!(
                                        "\x1b[200~[omnix] worker {who} ({agent_name}) kehabisan kuota{}.{} Periksa `omnix events --kind agent-quota-exhausted` dan teruskan ke cadangan.\x1b[201~\r",
                                        ex.reset_local
                                            .as_deref()
                                            .map(|r| format!(" (pulih {r})"))
                                            .unwrap_or_default(),
                                        wip_sha
                                            .as_deref()
                                            .map(|s| format!(" WIP commit: {s}."))
                                            .unwrap_or_default(),
                                    );
                                    let _ = orch_pty.writer.write_all(notice.as_bytes());
                                    let _ = orch_pty.writer.flush();
                                    crate::control::event(
                                        "agent-quota-action",
                                        json!({
                                            "handoff": if handoff_name.is_empty() { Value::Null } else { json!(handoff_name) },
                                            "pane": key,
                                            "agent": agent_name,
                                            "action": "notify-orchestrator",
                                            "targetPane": ok_key,
                                            "wipSha": wip_sha,
                                            "ok": true,
                                        }),
                                    );
                                }
                            }
                        }
                    }
                }
                chunks.entry(key).or_default().extend_from_slice(&data);
            }
            Msg::PtyExit { key } => exited.push(key),
            Msg::Probe(url) => probe(&app.ui, &url),
            Msg::StartTest => start_selftest(&app.ui, &app.root),
            Msg::Reply(id, result) => reply(&app.ui, id, result),
            Msg::DeepCheck => {
                let _ = app.ui.evaluate_script(DEEP_JS);
            }
            Msg::Finish(ok) => {
                println!("selftest: {}", if ok { "PASS" } else { "FAIL" });
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::process::exit(if ok { 0 } else { 1 });
            }
        }
    }
    for (key, data) in chunks {
        emit(
            &app.ui,
            "ptyData",
            &json!({ "paneKey": key, "dataBase64": base64(&data) }),
        );
    }
    for key in exited {
        let mut code = 0i64;
        if let Some(mut session) = app.ptys.remove(&key) {
            if let Ok(Some(status)) = session.child.try_wait() {
                code = status.exit_code() as i64;
            } else {
                // Reap after killing so no `[agy] <defunct>` is left behind.
                let _ = session.child.kill();
                if let Ok(status) = session.child.wait() {
                    code = status.exit_code() as i64;
                }
            }
        }
        app.pane_procs.remove(&key);
        app.panes.remove(&key);
        app.pane_tails.remove(&key);
        app.pane_quota_reported.remove(&key);
        app.pending_prompts.remove(&key);
        if let Some(meta) = app.panes.get_mut(&key) {
            meta.status = if code == 0 { "done" } else { "failed" }.into();
            meta.exit_code = Some(code);
            meta.updated_at = crate::control::epoch();
            let snapshot = meta.clone();
            let pane = app.ptys.contains_key(&key);
            crate::control::event(
                "agent-exited",
                json!({
                    "pane": key.clone(),
                    "handoff": snapshot.handoff,
                    "agent": snapshot.agent,
                    "title": snapshot.title,
                    "code": code,
                    "live": pane,
                }),
            );
        }
        persist_panes(app);
        emit(&app.ui, "ptyExit", &json!({ "paneKey": key, "code": code }));
    }
}

fn window_icon() -> Option<tao::window::Icon> {
    tao::window::Icon::from_rgba(LOGO_RGBA.to_vec(), LOGO_SIDE, LOGO_SIDE).ok()
}

pub fn run() -> Result<()> {
    // Wayland sessions report an unsupported window handle to wry; XWayland works.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("GDK_BACKEND").is_none() {
        // SAFETY: called before any GTK/WebKit initialisation and before threads start.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
    // The DMA-BUF renderer keeps compositing on the GPU. The fallback path
    // saves ~100 MB at startup but repaints the whole window per frame, which
    // made typing and scrolling stutter on a 2x display (measured ~10x more
    // CPU). OMNIX_NO_DMABUF=1 opts back into the lighter fallback for old
    // drivers that crash with DMA-BUF.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        && std::env::var_os("OMNIX_NO_DMABUF").is_some()
    {
        // SAFETY: same, before GTK/WebKit initialisation.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }

    let root = std::env::var_os("OMNIX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if let Err(err) = pty::install_guide(&root) {
        eprintln!("omnix: could not write agent guide: {err}");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let (tx, rx) = channel::<Msg>();

    let builder = WindowBuilder::new()
        .with_title("Omnix")
        .with_window_icon(window_icon())
        .with_inner_size(LogicalSize::new(1240.0, 820.0))
        .with_min_inner_size(LogicalSize::new(760.0, 480.0));
    /* Linux and Windows use the page as the title bar: one set of controls,
    no native frame above it. Dragging and edge resizing are wired through
    windowDrag/windowResize. macOS keeps its traffic lights and hides the
    custom buttons instead. */
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_decorations(false);
    let window = builder.build(&event_loop).context("create window")?;

    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    let (w, h) = (size.width, size.height);

    let tx_ipc = tx.clone();
    let proxy_ipc = proxy.clone();
    let tx_load = tx.clone();
    let proxy_load = proxy.clone();
    let started = std::cell::Cell::new(false);
    let ui_builder = WebViewBuilder::new()
        .with_bounds(px_rect(
            window.inner_size().width,
            window.inner_size().height,
        ))
        .with_custom_protocol("omnix".into(), |_id, req| serve(req))
        .with_on_page_load_handler(move |event, url| {
            if std::env::var_os("OMNIX_GUI_PROBE").is_some() {
                println!(
                    "page load loaded={:?} {url}",
                    matches!(event, wry::PageLoadEvent::Finished)
                );
            }
            if url.starts_with("omnix://") && matches!(event, wry::PageLoadEvent::Finished) {
                resample(&tx_load, &proxy_load);
                let _ = tx_load.send(Msg::Probe(url.clone()));
                let _ = proxy_load.send_event(UserEvent::Wake);
            }
            if matches!(event, wry::PageLoadEvent::Finished)
                && std::env::var_os("OMNIX_SELFTEST").is_some()
                && !started.replace(true)
            {
                let _ = tx_load.send(Msg::StartTest);
                let _ = proxy_load.send_event(UserEvent::Wake);
            }
        })
        .with_ipc_handler(move |req| {
            let _ = tx_ipc.send(Msg::Ui(req.body().clone()));
            let _ = proxy_ipc.send_event(UserEvent::Wake);
        })
        .with_devtools(cfg!(debug_assertions))
        .with_url("omnix://localhost/");

    /* build_as_child on X11 hosts the page in a separate GtkWindow, and some
    window managers never give that child keyboard focus: clicks work while
    every keystroke is dropped. Building into the tao window's own GtkBox
    keeps one focus chain and lets GTK stretch the page with the window;
    a gtk::Fixed leaves it stuck at its first allocation. */
    #[cfg(target_os = "linux")]
    let ui = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        let vbox = window
            .default_vbox()
            .context("tao window has no default vbox")?;
        ui_builder
            .build_gtk(vbox)
            .context("create ui webview: is WEBKIT_DISABLE_COMPOSITING_MODE needed?")?
    };
    #[cfg(not(target_os = "linux"))]
    let ui = ui_builder
        .build(&window)
        .context("create ui webview: is WEBKIT_DISABLE_COMPOSITING_MODE needed?")?;

    /* build_as_child leaves keyboard focus on the GtkWindow on some
    compositors; without this the pty never receives keystrokes. */
    let _ = ui.focus();

    let mut app = App {
        content: None,
        ui,
        window,
        root,
        size: (w, h),
        content_rect: (0.0, 0.0, w, h),
        content_visible: false,
        browser_ctx: None,
        ptys: HashMap::new(),
        pane_procs: HashMap::new(),
        panes: BTreeMap::new(),
        pane_tails: HashMap::new(),
        pane_quota_reported: HashSet::new(),
        pending_prompts: HashMap::new(),
        ctrl: None,
        quit: false,
    };

    let control_tx = tx.clone();
    let control_proxy = std::sync::Arc::new(std::sync::Mutex::new(proxy.clone()));
    match crate::control::start(
        &crate::control::runtime_dir(),
        &app.root,
        move |request| {
            let (reply_tx, reply_rx) = channel::<Value>();
            if control_tx
                .send(Msg::Control(request, reply_tx))
                .is_err()
            {
                return json!({ "ok": false, "error": "window is closing" });
            }
            if let Ok(proxy) = control_proxy.lock() {
                let _ = proxy.send_event(UserEvent::Wake);
            }
            reply_rx
                .recv_timeout(Duration::from_secs(60))
                .unwrap_or_else(|_| json!({ "ok": false, "error": "control timed out" }))
        },
    ) {
        Ok(server) => app.ctrl = Some(server),
        Err(err) => eprintln!("omnix: control socket disabled: {err}"),
    }
    persist_panes(&app);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::Wake) => {
                drain(&mut app, &rx, &tx, &proxy);
                if app.quit {
                    shutdown(&mut app);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => {
                let _ = app.ui.focus();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(physical),
                ..
            } => {
                let logical = physical.to_logical::<f64>(app.window.scale_factor());
                app.size = (logical.width, logical.height);
                let _ = app.ui.set_bounds(px_rect(physical.width, physical.height));
                if app.content.is_some() {
                    let (x, y, w, h) = app.content_rect;
                    let h = h.min((logical.height - y).max(1.0));
                    let w = w.min((logical.width - x).max(1.0));
                    app.content_rect = (x, y, w, h);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Destroyed,
                ..
            } => {
                shutdown(&mut app);
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                shutdown(&mut app);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}
