//! Discovery of the agent transcripts already on this machine.
//!
//! Every agent keeps its own format, so each one gets a small reader here and
//! everything is normalised into the same `history::Entry` rows:
//!
//! - Codex:  `~/.codex/sessions/**/rollout-*.jsonl`
//! - Claude: `~/.claude/projects/<slug>/<uuid>.jsonl` (subagents skipped)
//! - OpenCode: `~/.local/share/opencode/opencode.db` (sessions + messages)

use crate::history::{self, Entry, TranscriptLine};
use anyhow::{Context, Result};use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
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
    pub pruned: usize,
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
    usage: history::Usage,
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

/// Roots for every account: the base login plus each configured profile.
fn roots_for(root: &Path) -> Vec<(String, PathBuf, bool)> {
    let mut out = vec![
        ("codex".to_string(), codex_root(), true),
        ("claude".to_string(), claude_root(), false),
    ];
    for row in crate::agent::profile_rows(root) {
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        if row.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let base = row.get("base").and_then(Value::as_str).unwrap_or("");
        let Some(dir) = crate::agent::profile_dir_at(&crate::control::home(), root, name) else {
            continue;
        };
        match base {
            "codex" => out.push((name.to_string(), dir.join("sessions"), true)),
            "claude" => out.push((name.to_string(), dir.join("projects"), false)),
            _ => {}
        }
    }
    out
}

/// Roots that actually exist, for the GUI's "scanned N files" line.
pub fn roots(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for (agent, path, _) in roots_for(root) {
        if path.exists() && !out.iter().any(|(_, seen)| seen == &path) {
            out.push((agent, path));
        }
    }
    let db = opencode_db();
    if db.exists() {
        out.push(("opencode".to_string(), db));
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
    let mut usage = history::Usage::default();
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
                if payload.get("type").and_then(Value::as_str) == Some("token_count") {
                    /* Codex writes the running total after every turn; the
                       last snapshot is the session's usage. */
                    if let Some(total) = payload.pointer("/info/total_token_usage") {
                        let input = total.get("input_tokens").and_then(Value::as_i64).unwrap_or(0);
                        let cached = total
                            .get("cached_input_tokens")
                            .and_then(Value::as_i64)
                            .unwrap_or(0);
                        usage.input = (input - cached).max(0);
                        usage.cache_read = cached;
                        usage.cache_write = 0;
                        usage.output = total.get("output_tokens").and_then(Value::as_i64).unwrap_or(0);
                    }
                    continue;
                }
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
        usage,
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
    let mut usage = history::Usage::default();
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
        if kind == "assistant" {
            if let Some(reported) = message.get("usage") {
                usage.input += reported.get("input_tokens").and_then(Value::as_i64).unwrap_or(0);
                usage.cache_write += reported
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                usage.cache_read += reported
                    .get("cache_read_input_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                usage.output += reported.get("output_tokens").and_then(Value::as_i64).unwrap_or(0);
            }
            if let Some(model) = message.get("model").and_then(Value::as_str) {
                if !model.is_empty() {
                    usage.model = Some(model.to_string());
                }
            }
        }
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
        usage,
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
            usage: history::Usage::default(),
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
            usage: parsed.usage.clone(),
        },
    )?;
    report.add(&parsed.agent, stored);
    Ok(())
}

/// One scan at a time: the GUI boot scan and a manual rescan can overlap.
static SCAN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const USAGE_VERSION: &str = "1";

/// Existing indexes predate token counters. Read each transcript once more and
/// update only those columns, so the search index is not rebuilt.
fn backfill_usage(conn: &Connection, files: &[PathBuf], codex_roots: &[PathBuf]) -> Result<()> {
    if history::meta(conn, "usage_version").as_deref() == Some(USAGE_VERSION) {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "UPDATE sessions SET input_tokens=?1, cache_read_tokens=?2,
                cache_write_tokens=?3, output_tokens=?4, model=?5
         WHERE file_path=?6",
    )?;
    for path in files {
        let parsed = if codex_roots.iter().any(|root| path.starts_with(root)) {
            parse_codex(path)
        } else {
            parse_claude(path)
        };
        let Some(parsed) = parsed else { continue };
        stmt.execute(params![
            parsed.usage.input,
            parsed.usage.cache_read,
            parsed.usage.cache_write,
            parsed.usage.output,
            parsed.usage.model,
            path.to_string_lossy(),
        ])?;
    }
    history::set_meta(conn, "usage_version", USAGE_VERSION)?;
    Ok(())
}

/// Index every transcript found under the known agent roots.
pub fn scan(conn: &mut Connection, settings_root: &Path) -> Result<ScanReport> {
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut report = ScanReport::default();

    let roots = roots_for(settings_root);
    let codex_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|(_, _, is_codex)| *is_codex)
        .map(|(_, path, _)| path.clone())
        .collect();
    let mut sources: Vec<(PathBuf, String, bool)> = Vec::new();
    for (label, dir, is_codex) in &roots {
        let mut found = Vec::new();
        walk_jsonl(dir, &mut found);
        for path in found {
            if sources.iter().any(|(seen, _, _)| seen == &path) {
                continue;
            }
            sources.push((path, label.clone(), *is_codex));
        }
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    let files: Vec<PathBuf> = sources.iter().map(|(path, _, _)| path.clone()).collect();
    let keep: HashSet<String> = files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    let verbose = std::env::var_os("OMNIX_GUI_PROBE").is_some();
    if verbose {
        eprintln!("scan: {} jsonl files", files.len());
    }
    for (path, label, is_codex) in &sources {
        let key = path.to_string_lossy().to_string();
        if is_fresh(conn, &key, history::file_mtime(path)) {
            report.unchanged += 1;
            continue;
        }
        let parsed = if *is_codex {
            parse_codex(path)
        } else {
            parse_claude(path)
        };
        let Some(mut parsed) = parsed else {
            report.skipped += 1;
            continue;
        };
        parsed.agent = label.clone();
        store(conn, &key, parsed, &mut report)?;
        if verbose && report.files % 25 == 0 {
            eprintln!("scan: {} files, {} msgs", report.files, report.messages);
        }
    }

    backfill_usage(conn, &files, &codex_roots)?;

    let db = opencode_db();
    if db.exists() {
        // schema drift must not take the JSONL sources down with it
        if opencode_scan(conn, &db, &mut report).is_err() {
            report.skipped += 1;
        }
    }

    // Only prune agents whose roots exist here; a missing ~/.codex must not
    // wipe the sessions of someone who never installed Codex.
    let mut agents: Vec<String> = Vec::new();
    for (label, dir, _) in &roots {
        if dir.is_dir() && !agents.contains(label) {
            agents.push(label.clone());
        }
    }
    let agent_refs: Vec<&str> = agents.iter().map(String::as_str).collect();
    report.pruned = history::prune_missing(conn, &agent_refs, &keep)?;
    history::maintain(conn)?;

    Ok(report)
}
