use crate::{browser, handoff, history, pty, workspace};
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use wry::dpi::{LogicalPosition as WryPos, LogicalSize as WrySize};
use wry::{Rect, WebView, WebViewBuilder};

const INDEX_HTML: &str = include_str!("../app/index.html");
const APP_CSS: &str = include_str!("../app/style.css");
const APP_JS: &str = include_str!("../app/app.js");
const XTERM_JS: &str = include_str!("../app/vendor/xterm.js");
const XTERM_CSS: &str = include_str!("../app/vendor/xterm.css");
const ADDON_FIT_JS: &str = include_str!("../app/vendor/addon-fit.js");
const TUTORIAL: &str = include_str!("../tutorial/index.html");
const ICON: &str = include_str!("../app/icon.svg");

pub enum UserEvent {
    Wake,
}

enum Msg {
    Ui(String),
    Probe(String),
    StartTest,
    Finish(bool),
    PtyData { key: String, data: Vec<u8> },
    PtyExit { key: String },
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
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: WryPos::new(x, y).into(),
        size: WrySize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn asset(path: &str) -> Option<(&'static str, &'static str)> {
    Some(match path {
        "/" | "/index.html" => (INDEX_HTML, "text/html; charset=utf-8"),
        "/style.css" => (APP_CSS, "text/css; charset=utf-8"),
        "/app.js" => (APP_JS, "text/javascript; charset=utf-8"),
        "/vendor/xterm.js" => (XTERM_JS, "text/javascript; charset=utf-8"),
        "/vendor/xterm.css" => (XTERM_CSS, "text/css; charset=utf-8"),
        "/vendor/addon-fit.js" => (ADDON_FIT_JS, "text/javascript; charset=utf-8"),
        "/tutorial.html" => (TUTORIAL, "text/html; charset=utf-8"),
        "/icon.svg" => (ICON, "image/svg+xml"),
        _ => return None,
    })
}

fn serve(req: wry::http::Request<Vec<u8>>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    let path = req.uri().path().to_string();
    match asset(&path) {
        Some((body, kind)) => wry::http::Response::builder()
            .header("Content-Type", kind)
            .header("Cache-Control", "no-store")
            .body(std::borrow::Cow::Owned(body.as_bytes().to_vec()))
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
        "appInfo" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "root": app.root.to_string_lossy(),
            "engine": { "lightpanda": browser::available(), "version": browser::version() },
            "platform": std::env::consts::OS,
        })),
        "status" => (|| {
            let db = history_db(&app.root);
            let conn = history::open(&db)?;
            let (sessions, messages, bytes) = history::status(&conn, &db)?;
            let workspaces = workspace::list(&app.root)?;
            Ok(json!({
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
        "agentTrack" => (|| {
            let agent = as_str(&args, "agent").ok_or_else(|| anyhow!("agent is required"))?;
            let session = as_str(&args, "session").ok_or_else(|| anyhow!("session is required"))?;
            let pane = as_str(&args, "paneKey").unwrap_or_else(|| "pane:1".into());
            let cwd =
                as_str(&args, "cwd").unwrap_or_else(|| app.root.to_string_lossy().to_string());
            let record = workspace::track(
                &app.root,
                &app.root,
                &agent,
                &session,
                &PathBuf::from(&cwd),
                &pane,
            )?;
            Ok(json!({
                "paneKey": record.pane_key,
                "agent": record.agent,
                "sessionId": record.session_id,
                "cwd": record.cwd,
                "resumeCommand": record.resume_command,
            }))
        })(),
        "sleep" => (|| {
            let ws = as_str(&args, "workspace")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.root.clone());
            let state = workspace::sleep(&app.root, &ws)?;
            Ok(json!({ "path": state.path, "sessions": state.sessions.len() }))
        })(),
        "resume" => (|| {
            let ws = as_str(&args, "workspace")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.root.clone());
            let state = workspace::wake(&app.root, &ws, false)?;
            Ok(json!({ "path": state.path, "sessions": state.sessions }))
        })(),
        "handoff" => (|| {
            let name = as_str(&args, "name").ok_or_else(|| anyhow!("name is required"))?;
            let prompt = as_str(&args, "prompt").ok_or_else(|| anyhow!("prompt is required"))?;
            let agent = as_str(&args, "agent").unwrap_or_else(|| "codex".into());
            let base = as_str(&args, "base");
            let (record, _) =
                handoff::create(&app.root, &name, &agent, &prompt, base.as_deref(), false)?;
            Ok(json!({
                "outcome": record.outcome,
                "worktree": record.worktree,
                "branch": record.branch,
                "brief": record.brief,
                "launch": format!("cd {} && {}", record.worktree, record.agent),
            }))
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
            let spawned = match (agent, session) {
                (Some(a), Some(s)) => pty::spawn(shell.as_deref(), &cwd, Some((&a, &s)))?,
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
            Ok(json!({ "ok": true }))
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

/// Debug aid: proves the embedded UI loaded and its JS parsed.
fn probe(ui: &WebView, url: &str) {
    if std::env::var_os("OMNIX_GUI_PROBE").is_none() {
        return;
    }
    let js = "(function(){try{var d=document.documentElement;return JSON.stringify({title:document.title,\
        nodes:document.querySelectorAll('*').length,bridge:typeof window.__omnix,\
        overflowX:d.scrollWidth>d.clientWidth,overflowY:d.scrollHeight>d.clientHeight,\
        vw:d.clientWidth,vh:d.clientHeight,\
        rail:(document.getElementById('rail')||{}).offsetWidth||0,\
        side:(document.getElementById('sidebar')||{}).offsetWidth||0,\
        visible:Array.from(document.querySelectorAll('button')).filter(function(b){return b.offsetParent}).length});}catch(e){return JSON.stringify({error:String(e)})}})()";
    let url = url.to_string();
    let _ = ui.evaluate_script_with_callback(js, move |result| {
        println!("probe {url}: {result}");
    });
}

fn start_selftest(ui: &WebView) {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(90));
        eprintln!("selftest: TIMEOUT");
        std::process::exit(1);
    });
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
            Msg::StartTest => start_selftest(&app.ui),
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
        emit(&app.ui, "ptyExit", &json!({ "paneKey": key, "code": code }));
    }
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

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let (tx, rx) = channel::<Msg>();

    let window = WindowBuilder::new()
        .with_title("Omnix")
        .with_inner_size(LogicalSize::new(1240.0, 820.0))
        .with_min_inner_size(LogicalSize::new(760.0, 480.0))
        .build(&event_loop)
        .context("create window")?;

    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    let (w, h) = (size.width, size.height);

    let tx_ipc = tx.clone();
    let proxy_ipc = proxy.clone();
    let tx_load = tx.clone();
    let proxy_load = proxy.clone();
    let started = std::cell::Cell::new(false);
    let ui = WebViewBuilder::new()
        .with_bounds(rect(0.0, 0.0, w, h))
        .with_custom_protocol("omnix".into(), |_id, req| serve(req))
        .with_on_page_load_handler(move |event, url| {
            if std::env::var_os("OMNIX_GUI_PROBE").is_some() {
                println!(
                    "page load loaded={:?} {url}",
                    matches!(event, wry::PageLoadEvent::Finished)
                );
            }
            if url.starts_with("omnix://") {
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
        .with_url("omnix://localhost/")
        .build_as_child(&window)
        .context("create ui webview: is WEBKIT_DISABLE_COMPOSITING_MODE needed?")?;

    let mut app = App {
        content: None,
        ui,
        window,
        root,
        size: (w, h),
        content_rect: (0.0, 0.0, w, h),
        content_visible: false,
        ptys: HashMap::new(),
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::Wake) => drain(&mut app, &rx, &tx, &proxy),
            Event::WindowEvent {
                event: WindowEvent::Resized(physical),
                ..
            } => {
                let logical = physical.to_logical::<f64>(app.window.scale_factor());
                app.size = (logical.width, logical.height);
                let _ = app
                    .ui
                    .set_bounds(rect(0.0, 0.0, logical.width, logical.height));
                if app.content.is_some() {
                    let (x, y, w, h) = app.content_rect;
                    let h = h.min((logical.height - y).max(1.0));
                    let w = w.min((logical.width - x).max(1.0));
                    app.content_rect = (x, y, w, h);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                for (_, mut session) in app.ptys.drain() {
                    let _ = session.child.kill();
                }
                app.content = None;
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}
