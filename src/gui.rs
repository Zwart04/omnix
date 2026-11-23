use crate::{browser, history, procs, projects, pty, sources, update, workspace};
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
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
const LOGO_PNG: &[u8] = include_bytes!("../app/logo.png");
const LOGO_RGBA: &[u8] = include_bytes!("../app/logo.rgba");
const LOGO_SIDE: u32 = 128;

pub enum UserEvent {
    Wake,
}

enum Msg {
    Ui(String),
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

struct App {
    content: Option<WebView>,
    ui: WebView,
    window: Window,
    root: PathBuf,
    size: (f64, f64),
    content_rect: (f64, f64, f64, f64),
    content_visible: bool,
    ptys: HashMap<String, pty::Pty>,
    pane_procs: HashMap<String, PaneProc>,
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
        "/logo.png" => (LOGO_PNG, "image/png"),
        _ => return None,
    })
}

fn serve(req: wry::http::Request<Vec<u8>>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let path = req.uri().path().to_string();
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

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn history_db(root: &PathBuf) -> PathBuf {
    root.join(".omnix").join("history.sqlite")
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
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "root": app.root.to_string_lossy(),
                "engine": {
                    "lightpanda": engine_version.is_some(),
                    "version": engine_version,
                },
                "platform": std::env::consts::OS,
                "traceKeys": app.root.join(".trace-keys").exists(),
                "agents": crate::agent::AGENTS
                    .iter()
                    .map(|a| json!({
                        "name": a,
                        "bin": crate::agent::launch_cmd(a),
                        "available": crate::agent::available(a),
                        "enabled": saved.get(*a).map(|c| c.enabled).unwrap_or(true),
                    }))
                    .collect::<Vec<_>>(),
            })
        }),
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
        "usage" => Ok(crate::usage::snapshot(home_dir().as_path())),
        "agentSettings" => (|| {
            let saved = crate::agent::load_settings(&app.root);
            let defaults = crate::agent::load_defaults(&app.root);
            Ok(json!({
                "path": crate::agent::settings_path(&app.root).to_string_lossy(),
                "defaults": { "fullAccess": defaults.full_access },
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
                    .collect::<Vec<_>>(),
            }))
        })(),
        "agentSettingsSave" => (|| {
            let name = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if !crate::agent::AGENTS.contains(&name.as_str()) {
                return Err(anyhow!("unknown agent '{name}'"));
            }
            let extra_args = as_str(&args, "args").unwrap_or_default();
            let env = as_str(&args, "env").unwrap_or_default();
            if extra_args.len() > 400 || env.len() > 400 {
                return Err(anyhow!("agent settings are too long"));
            }
            let mut saved = crate::agent::load_settings(&app.root);
            let mut cfg = saved.remove(&name).unwrap_or_default();
            cfg.args = extra_args;
            cfg.env = env;
            if cfg.args.trim().is_empty() && cfg.env.trim().is_empty() && cfg.enabled {
                // nothing custom left: fall back to the implicit defaults
            } else {
                saved.insert(name.clone(), cfg);
            }
            crate::agent::save_settings(&app.root, &saved)?;
            Ok(json!({ "ok": true }))
        })(),
        "agentEnable" => (|| {
            let name = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            if !crate::agent::AGENTS.contains(&name.as_str()) {
                return Err(anyhow!("unknown agent '{name}'"));
            }
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("enabled must be a boolean"))?;
            let mut saved = crate::agent::load_settings(&app.root);
            let mut cfg = saved.remove(&name).unwrap_or_default();
            cfg.enabled = enabled;
            if cfg.args.trim().is_empty() && cfg.env.trim().is_empty() && enabled {
                // enabled is the default; keep the store empty
            } else {
                saved.insert(name.clone(), cfg);
            }
            crate::agent::save_settings(&app.root, &saved)?;
            Ok(json!({ "ok": true, "enabled": enabled }))
        })(),
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
                    let report = sources::scan(&mut conn)?;
                    if probe_on {
                        eprintln!("scan: walked {} files", report.files);
                    }
                    let counts = history::agent_counts(&conn)?;
                    Ok(json!({
                        "files": report.files,
                        "messages": report.messages,
                        "skipped": report.skipped,
                        "unchanged": report.unchanged,
                        "db": db.to_string_lossy(),
                        "agents": counts
                            .iter()
                            .map(|c| json!({"agent": c.agent, "sessions": c.sessions, "messages": c.messages}))
                            .collect::<Vec<_>>(),
                        "roots": sources::roots()
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
            let sessions: Vec<Value> = rows
                .into_iter()
                .map(|r| {
                    json!({
                        "agent": r.agent,
                        "sessionId": r.session_id,
                        "title": r.title,
                        "cwd": r.cwd,
                        "updatedAt": r.updated_at,
                        "messageCount": r.message_count,
                        "resumeCommand": r.resume_command,
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
            let spawned = match (agent, session, command) {
                (_, _, Some(line)) => {
                    if line.len() > 240 {
                        return Err(anyhow!("command is too long"));
                    }
                    pty::spawn_command(&line, &cwd)?
                }
                (Some(a), s, None) => pty::spawn_agent(&app.root, &a, s.as_deref(), &cwd)?,
                _ => pty::spawn(shell.as_deref(), &cwd, None)?,
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
                    label,
                    cwd: cwd.to_string_lossy().into_owned(),
                },
            );
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
            Ok(json!({ "ok": true }))
        })(),
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
                .map(|mb| mb.saturating_mul(1024 * 1024));
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
                    let view = WebViewBuilder::new()
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
            browser::stop()?;
            Ok(json!({ "ok": true }))
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
                "preview": text.chars().take(1400).collect::<String>(),
            }))
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
    app.content = None;
}

fn drain(app: &mut App, rx: &Receiver<Msg>, tx: &Sender<Msg>, proxy: &EventLoopProxy<UserEvent>) {
    let mut chunks: HashMap<String, Vec<u8>> = HashMap::new();
    let mut exited: Vec<String> = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Msg::Ui(raw) => handle(app, &raw, tx, proxy),
            Msg::PtyData { key, data } => {
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
            }
            let _ = session.child.kill();
        }
        app.pane_procs.remove(&key);
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
    every keystroke is dropped. Hosting the page in the tao window's own
    GtkBox with a gtk::Fixed keeps one focus chain. */
    #[cfg(target_os = "linux")]
    let ui = {
        use gtk::prelude::*;
        use tao::platform::unix::WindowExtUnix;
        use wry::{WebViewBuilderExtUnix, WebViewExtUnix};

        let fixed = gtk::Fixed::new();
        let vbox = window
            .default_vbox()
            .context("tao window has no default vbox")?;
        vbox.pack_start(&fixed, true, true, 0);
        fixed.show_all();
        let view = ui_builder
            .build_gtk(&fixed)
            .context("create ui webview: is WEBKIT_DISABLE_COMPOSITING_MODE needed?")?;
        view.webview().set_hexpand(true);
        view.webview().set_vexpand(true);
        view
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
        ptys: HashMap::new(),
        pane_procs: HashMap::new(),
        quit: false,
    };

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
