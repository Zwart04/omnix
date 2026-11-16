//! Discovery of the agent transcripts already on this machine.
//!
//! Every agent keeps its own format, so each one gets a small reader here and
//! everything is normalised into the same `history::Entry` rows:
//!
//! - Codex:  `~/.codex/sessions/**/rollout-*.jsonl`
//! - Claude: `~/.claude/projects/<slug>/<uuid>.jsonl` (subagents skipped)
//! - OpenCode: `~/.local/share/opencode/opencode.db` (sessions + messages)

use crate::history::{self, Entry, TranscriptLine};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One transcript can be hundreds of megabytes of tool output; a single
/// gigantic file must never stall the scan or blow up the FTS index.
const MAX_LINE_BYTES: usize = 1 << 20;
const MAX_LINES_PER_FILE: usize = 12_000;
/// A single transcript can reach hundreds of megabytes; indexing its head keeps
/// the scan bounded and still yields the title plus a usable resume command.
const MAX_FILE_BYTES: u64 = 16 << 20;

#[derive(Debug, Default)]
pub struct ScanReport {
    pub files: usize,
    pub messages: usize,
    pub skipped: usize,
    /// agent -> (sessions, messages)
    pub agents: BTreeMap<String, (usize, usize)>,
    pub unchanged: usize,
}

impl ScanReport {
    fn add(&mut self, agent: &str, messages: usize) {
        let slot = self.agents.entry(agent.to_string()).or_insert((0, 0));
        slot.0 += 1;
        slot.1 += messages;
        self.files += 1;
        self.messages += messages;
    }
}

struct Parsed {
    agent: String,
    session_id: String,
    cwd: Option<String>,
    updated_at: i64,
    lines: Vec<TranscriptLine>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn codex_root() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".codex"))
        .join("sessions")
}

fn claude_root() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
        .join("projects")
}

fn opencode_db() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("opencode")
        .join("opencode.db")
}

/// Roots that actually exist, for the GUI's "scanned N files" line.
pub fn roots() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (agent, path) in [
        ("codex", codex_root()),
        ("claude", claude_root()),
        ("opencode", opencode_db()),
    ] {
        if path.exists() {
            out.push((agent.to_string(), path));
        }
    }
    out
}

fn walk_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|s| s.to_str()) == Some("subagents") {
                continue;
            }
            walk_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(text.clone()),
                Value::Object(map) => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|_| {
                        matches!(
                            map.get("type").and_then(Value::as_str),
                            Some("text" | "Text" | "input_text" | "output_text")
                        )
                    }),
                _ => None,
            })
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Codex rollouts keep user/assistant text in `event_msg` items.
fn parse_codex(path: &Path) -> Option<Parsed> {
    let reader = std::io::Read::take(
        std::io::BufReader::new(fs::File::open(path).ok()?),
        MAX_FILE_BYTES,
    );
    let mut cwd = None;
    let mut session_id = None;
    let mut lines = Vec::new();
    let mut seen = 0usize;

    for line in std::io::BufRead::lines(reader) {
        let Ok(line) = line else { continue };
        if seen >= MAX_LINES_PER_FILE {
            break;
        }
        seen += 1;
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                let payload = &value["payload"];
                cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(cwd);
                session_id = payload
                    .get("session_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(session_id);
            }
            Some("response_item") => {
                let payload = &value["payload"];
                if payload.get("type").and_then(Value::as_str) != Some("message") {
                    continue;
                }
                let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
                if !matches!(role, "user" | "assistant") {
                    continue;
                }
                let text = text_of(&payload["content"]);
                if text.trim().is_empty() {
                    continue;
                }
                lines.push(TranscriptLine {
                    role: role.to_string(),
                    text,
                    ts: value
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            Some("event_msg") => {
                let payload = &value["payload"];
                if payload.get("type").and_then(Value::as_str) != Some("item_completed") {
                    continue;
                }
                let item = &payload["item"];
                let role = match item.get("type").and_then(Value::as_str) {
                    Some("UserMessage") => "user",
                    Some("AgentMessage") => "assistant",
                    _ => continue,
                };
                let text = text_of(&item["content"]);
                if text.trim().is_empty() {
                    continue;
                }
                lines.push(TranscriptLine {
                    role: role.to_string(),
                    text,
                    ts: value
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            _ => {}
        }
    }

    if lines.is_empty() {
        return None;
    }
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session");
    let session_id = session_id.unwrap_or_else(|| {
        fallback
            .rsplit('-')
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("-")
    });
    Some(Parsed {
        agent: "codex".into(),
        session_id,
        cwd,
        updated_at: history::file_mtime(path),
        lines,
    })
}

fn parse_claude(path: &Path) -> Option<Parsed> {
    let reader = std::io::Read::take(
        std::io::BufReader::new(fs::File::open(path).ok()?),
        MAX_FILE_BYTES,
    );
    let mut cwd = None;
    let mut session_id = None;
    let mut lines = Vec::new();
    let mut seen = 0usize;

    for line in std::io::BufRead::lines(reader) {
        let Ok(line) = line else { continue };
        if seen >= MAX_LINES_PER_FILE {
            break;
        }
        seen += 1;
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(dir) = value.get("cwd").and_then(Value::as_str) {
            cwd = Some(dir.to_string());
        }
        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            session_id = Some(id.to_string());
        }
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(kind, "user" | "assistant") {
            continue;
        }
        if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let message = &value["message"];
        let text = text_of(&message["content"]);
        if text.trim().is_empty() {
            continue;
        }
        lines.push(TranscriptLine {
            role: kind.to_string(),
            text,
            ts: value
                .get("timestamp")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }

    if lines.is_empty() {
        return None;
    }
    let session_id = session_id.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    })?;
    Some(Parsed {
        agent: "claude".into(),
        session_id,
        cwd,
        updated_at: history::file_mtime(path),
        lines,
    })
}

struct OpencodeMeta {
    id: String,
    cwd: Option<String>,
    updated_at: i64,
}

/// OpenCode keeps everything in SQLite; read-only, never write that database.
fn opencode_scan(conn: &mut Connection, db: &Path, report: &mut ScanReport) -> Result<()> {
    let src = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("open opencode.db read-only")?;

    let mut meta_stmt = src.prepare("SELECT id, directory, time_updated FROM session_v2")?;
    let metas: Vec<OpencodeMeta> = meta_stmt
        .query_map([], |row| {
            Ok(OpencodeMeta {
                id: row.get(0)?,
                cwd: row.get(1)?,
                updated_at: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            })
        })?
        .filter_map(Result::ok)
        .collect();
    drop(meta_stmt);

    let mut messages = src.prepare(
        "SELECT type, data, time_created FROM session_message
         WHERE session_id = ?1 AND type IN ('user','assistant') ORDER BY seq",
    )?;

    for meta in metas {
        let key = format!("opencode://{}", meta.id);
        let updated_at = meta.updated_at / 1000;
        if is_fresh(conn, &key, updated_at) {
            report.unchanged += 1;
            continue;
        }
        let rows = messages.query_map([&meta.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        let mut lines = Vec::new();
        for row in rows.filter_map(Result::ok) {
            let (kind, data, created) = row;
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let text = match value.get("content") {
                Some(content) => text_of(content),
                None => value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            if text.trim().is_empty() {
                continue;
            }
            lines.push(TranscriptLine {
                role: kind,
                text,
                ts: Some(created.to_string()),
            });
        }
        if lines.is_empty() {
            continue;
        }
        let parsed = Parsed {
            agent: "opencode".into(),
            session_id: meta.id,
            cwd: meta.cwd,
            updated_at,
            lines,
        };
        store(conn, &key, parsed, report)?;
    }
    Ok(())
}

fn is_fresh(conn: &Connection, key: &str, updated_at: i64) -> bool {
    conn.query_row(
        "SELECT updated_at FROM sessions WHERE file_path = ?1 AND message_count > 0",
        [key],
        |row| row.get::<_, i64>(0),
    )
    .map(|stored| stored == updated_at)
    .unwrap_or(false)
}

fn store(conn: &mut Connection, key: &str, parsed: Parsed, report: &mut ScanReport) -> Result<()> {
    if is_fresh(conn, key, parsed.updated_at) {
        report.unchanged += 1;
        return Ok(());
    }
    let title = history::session_title(&parsed.lines, &parsed.session_id);
    let resume_command =
        crate::agent::resume_command(&parsed.agent, &parsed.session_id).unwrap_or_default();
    let stored = history::upsert(
        conn,
        &Entry {
            key,
            agent: &parsed.agent,
            session_id: &parsed.session_id,
            title: &title,
            cwd: parsed.cwd.as_deref(),
            updated_at: parsed.updated_at,
            resume_command: &resume_command,
            hash: "",
            lines: &parsed.lines,
        },
    )?;
    report.add(&parsed.agent, stored);
    Ok(())
}

/// One scan at a time: the GUI boot scan and a manual rescan can overlap.
static SCAN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Index every transcript found under the known agent roots.
pub fn scan(conn: &mut Connection) -> Result<ScanReport> {
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut report = ScanReport::default();

    let mut files = Vec::new();
    walk_jsonl(&codex_root(), &mut files);
    walk_jsonl(&claude_root(), &mut files);
    files.sort();

    let verbose = std::env::var_os("OMNIX_GUI_PROBE").is_some();
    if verbose {
        eprintln!("scan: {} jsonl files", files.len());
    }
    let codex = codex_root();
    for path in files {
        let key = path.to_string_lossy().to_string();
        if is_fresh(conn, &key, history::file_mtime(&path)) {
            report.unchanged += 1;
            continue;
        }
        let is_codex = path.starts_with(&codex);
        let parsed = if is_codex {
            parse_codex(&path)
        } else {
            parse_claude(&path)
        };
        let Some(parsed) = parsed else {
            report.skipped += 1;
            continue;
        };
        store(conn, &key, parsed, &mut report)?;
        if verbose && report.files % 25 == 0 {
            eprintln!("scan: {} files, {} msgs", report.files, report.messages);
        }
    }

    let db = opencode_db();
    if db.exists() {
        // schema drift must not take the JSONL sources down with it
        if opencode_scan(conn, &db, &mut report).is_err() {
            report.skipped += 1;
        }
    }

    Ok(report)
}
