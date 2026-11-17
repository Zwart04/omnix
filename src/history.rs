use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const SCHEMA_VERSION: i64 = 1;
const MAX_MESSAGE_CHARS: usize = 8_000;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sessions(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  file_path TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  cwd TEXT,
  updated_at INTEGER,
  message_count INTEGER NOT NULL DEFAULT 0,
  resume_command TEXT NOT NULL,
  content_hash TEXT
);
CREATE INDEX IF NOT EXISTS sessions_agent ON sessions(agent);
CREATE INDEX IF NOT EXISTS sessions_updated_at ON sessions(updated_at);
CREATE TABLE IF NOT EXISTS messages(
  id INTEGER PRIMARY KEY,
  session_row_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  ts TEXT,
  text_truncated INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS messages_session ON messages(session_row_id);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  user_text, assistant_text, tool_text, identifiers,
  tokenize="unicode61 tokenchars '_.-/+'", detail=full
);
"#;

#[derive(Deserialize)]
pub struct TranscriptLine {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Debug)]
pub struct IndexReport {
    pub files: usize,
    pub messages: usize,
    pub skipped_lines: usize,
}

pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    // Switching journal mode needs an exclusive lock and does not retry, so a
    // concurrent writer must not turn "open" into a hard failure.
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Panel reads and a background rescan can overlap; wait instead of failing.
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT INTO meta(key,value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value <> excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(conn)
}

fn jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            jsonl_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
    Ok(())
}

fn collapse(s: &str, max: usize) -> String {
    let mut out = String::new();
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() >= max {
            break;
        }
    }
    out.chars().take(max).collect()
}

fn camel_split(word: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in word.chars() {
        if c.is_uppercase() && prev_lower && !current.is_empty() {
            parts.push(current.clone());
            current.clear();
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// FTS-only identifiers: `resolveTerminalPath` also matches `resolve terminal path`.
fn identifiers(text: &str) -> String {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '/'))) {
        for part in raw.split(|c: char| matches!(c, '_' | '.' | '-' | '/')) {
            if part.len() < 2 {
                continue;
            }
            for token in camel_split(part) {
                if token.len() >= 2 && seen.insert(token.clone()) {
                    out.push(token);
                    if out.len() >= 4_000 {
                        return out.join(" ");
                    }
                }
            }
        }
    }
    out.join(" ")
}

fn content_hash(lines: &[TranscriptLine]) -> String {
    let mut hash = Sha256::new();
    for line in lines.iter().take(8) {
        hash.update(b"\0");
        hash.update(line.role.as_bytes());
        hash.update(b"\0");
        hash.update(line.text.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

pub fn index_dir(
    conn: &mut Connection,
    dir: &Path,
    agent_override: Option<&str>,
    cwd: &str,
) -> Result<IndexReport> {
    let mut files = Vec::new();
    jsonl_files(dir, &mut files)?;
    files.sort();

    let mut report = IndexReport {
        files: 0,
        messages: 0,
        skipped_lines: 0,
    };

    for path in files {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read transcript {}", path.display()))?;
        let mut lines = Vec::new();
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TranscriptLine>(line) {
                Ok(parsed) => lines.push(parsed),
                Err(_) => report.skipped_lines += 1,
            }
        }
        if lines.is_empty() {
            continue;
        }

        let file_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session");
        let agent = agent_override
            .map(str::to_string)
            .or_else(|| {
                path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .filter(|s| super::agent::launch_cmd(s).is_some())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "unknown".into());

        let title = session_title(&lines, file_name);
        let resume_command = super::agent::resume_command(&agent, file_name).unwrap_or_default();
        let updated_at = file_mtime(&path);
        let hash = content_hash(&lines);

        report.messages += upsert(
            conn,
            &Entry {
                key: &path.to_string_lossy(),
                agent: &agent,
                session_id: file_name,
                title: &title,
                cwd: Some(cwd),
                updated_at,
                resume_command: &resume_command,
                hash: &hash,
                lines: &lines,
            },
        )?;
        report.files += 1;
    }
    Ok(report)
}

#[derive(Debug)]
pub struct SessionRow {
    pub agent: String,
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub updated_at: Option<i64>,
    pub message_count: i64,
    pub resume_command: String,
    pub preview: String,
}

/// Session browser for the right-hand history panel.
pub fn list(
    conn: &Connection,
    agent: Option<&str>,
    cwd_prefix: Option<&str>,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<SessionRow>> {
    let mut sql = String::from(
        "SELECT s.agent, s.session_id, s.title, s.cwd, s.updated_at, s.message_count,
                s.resume_command,
                COALESCE((SELECT substr(f.user_text, 1, 220) FROM messages m
                          JOIN messages_fts f ON f.rowid = m.id
                          WHERE m.session_row_id = s.id AND m.role = 'user'
                          ORDER BY m.id LIMIT 1), '')
         FROM sessions s WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(agent) = agent.filter(|a| !a.is_empty() && *a != "all") {
        sql.push_str(" AND s.agent = ?");
        params.push(Box::new(agent.to_string()));
    }
    if let Some(prefix) = cwd_prefix.filter(|p| !p.is_empty()) {
        sql.push_str(" AND (s.cwd = ? OR s.cwd LIKE ? || '/%')");
        params.push(Box::new(prefix.to_string()));
        params.push(Box::new(prefix.to_string()));
    }
    if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
        sql.push_str(" AND (s.title LIKE ? OR s.session_id LIKE ? OR s.cwd LIKE ?)");
        let like = format!("%{}%", query.trim());
        params.push(Box::new(like.clone()));
        params.push(Box::new(like.clone()));
        params.push(Box::new(like));
    }
    sql.push_str(" ORDER BY s.updated_at DESC, s.id DESC LIMIT ?");
    params.push(Box::new(limit.clamp(1, 500) as i64));

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(SessionRow {
            agent: row.get(0)?,
            session_id: row.get(1)?,
            title: row.get(2)?,
            cwd: row.get(3)?,
            updated_at: row.get(4)?,
            message_count: row.get(5)?,
            resume_command: row.get(6)?,
            preview: row
                .get::<_, String>(7)?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[derive(Debug)]
pub struct ProjectRow {
    pub path: String,
    pub sessions: i64,
    pub messages: i64,
    pub updated_at: Option<i64>,
    pub agents: String,
}

/// Distinct working directories, the way the left project tree groups sessions.
pub fn projects(conn: &Connection, limit: usize) -> Result<Vec<ProjectRow>> {
    let mut stmt = conn.prepare(
        "SELECT cwd, COUNT(*), COALESCE(SUM(message_count), 0), MAX(updated_at),
                GROUP_CONCAT(DISTINCT agent)
         FROM sessions WHERE cwd IS NOT NULL AND cwd <> ''
         GROUP BY cwd ORDER BY MAX(updated_at) DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit.clamp(1, 300) as i64], |row| {
        Ok(ProjectRow {
            path: row.get(0)?,
            sessions: row.get(1)?,
            messages: row.get(2)?,
            updated_at: row.get(3)?,
            agents: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[derive(Debug)]
pub struct AgentCount {
    pub agent: String,
    pub sessions: i64,
    pub messages: i64,
}

pub fn agent_counts(conn: &Connection) -> Result<Vec<AgentCount>> {
    let mut stmt = conn.prepare(
        "SELECT agent, COUNT(*), COALESCE(SUM(message_count), 0)
         FROM sessions GROUP BY agent ORDER BY COUNT(*) DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(AgentCount {
            agent: row.get(0)?,
            sessions: row.get(1)?,
            messages: row.get(2)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub struct Entry<'a> {
    pub key: &'a str,
    pub agent: &'a str,
    pub session_id: &'a str,
    pub title: &'a str,
    pub cwd: Option<&'a str>,
    pub updated_at: i64,
    pub resume_command: &'a str,
    pub hash: &'a str,
    pub lines: &'a [TranscriptLine],
}

pub fn session_title(lines: &[TranscriptLine], fallback: &str) -> String {
    lines
        .iter()
        .find(|l| l.role == "user")
        .map(|l| collapse(&l.text, 80))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn file_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One row per transcript/session: replace messages, keep the session row fresh.
pub fn upsert(conn: &mut Connection, entry: &Entry) -> Result<usize> {
    // IMMEDIATE takes the write lock up front: a deferred transaction that reads
    // first gets SQLITE_BUSY_SNAPSHOT at once and would ignore busy_timeout.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let old_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM sessions WHERE file_path = ?1",
            params![entry.key],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = old_id {
        tx.execute(
            "DELETE FROM messages_fts WHERE rowid IN (SELECT id FROM messages WHERE session_row_id = ?1)",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM messages WHERE session_row_id = ?1",
            params![id],
        )?;
    }
    tx.execute(
        r#"INSERT INTO sessions(agent,session_id,file_path,title,cwd,updated_at,message_count,resume_command,content_hash)
           VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
           ON CONFLICT(file_path) DO UPDATE SET
             agent=excluded.agent, session_id=excluded.session_id, title=excluded.title,
             cwd=excluded.cwd, updated_at=excluded.updated_at,
             message_count=excluded.message_count, resume_command=excluded.resume_command,
             content_hash=excluded.content_hash"#,
        params![
            entry.agent,
            entry.session_id,
            entry.key,
            entry.title,
            entry.cwd,
            entry.updated_at,
            entry.lines.len() as i64,
            entry.resume_command,
            entry.hash
        ],
    )?;
    let session_row: i64 = tx.query_row(
        "SELECT id FROM sessions WHERE file_path = ?1",
        params![entry.key],
        |r| r.get(0),
    )?;

    let mut insert_msg = tx.prepare(
        "INSERT INTO messages(session_row_id, role, ts, text_truncated) VALUES(?1,?2,?3,?4)",
    )?;
    let mut insert_fts = tx.prepare(
        "INSERT INTO messages_fts(rowid,user_text,assistant_text,tool_text,identifiers) VALUES(?1,?2,?3,?4,?5)",
    )?;
    let mut stored = 0usize;
    for line in entry.lines {
        let truncated = line.text.chars().count() > MAX_MESSAGE_CHARS;
        let text: String = line.text.chars().take(MAX_MESSAGE_CHARS).collect();
        insert_msg.execute(params![
            session_row,
            line.role,
            line.ts,
            if truncated { 1 } else { 0 }
        ])?;
        let id = tx.last_insert_rowid();
        let (user, assistant, tool) = match line.role.as_str() {
            "user" => (text.as_str(), "", ""),
            "assistant" => ("", text.as_str(), ""),
            "tool" => ("", "", text.as_str()),
            _ => ("", "", ""),
        };
        insert_fts.execute(params![id, user, assistant, tool, identifiers(&text)])?;
        stored += 1;
    }
    drop(insert_msg);
    drop(insert_fts);
    tx.commit()?;
    Ok(stored)
}

#[derive(Debug)]
pub struct Hit {
    pub agent: String,
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub updated_at: Option<i64>,
    pub snippet: String,
    pub resume_command: String,
    pub score: f64,
}

/// Tokenizer keeps `.` inside tokens, so `resolveTerminalPath.` is one term.
/// Strip edge punctuation from the query side to match the stored token.
fn clean_term(term: &str) -> &str {
    term.trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | '+')))
}

fn quoted_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(clean_term)
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect()
}

fn attempts(query: &str) -> Vec<String> {
    let terms = quoted_terms(query);
    let mut out = Vec::new();
    if terms.len() > 1 {
        out.push(terms.join(" "));
        out.push(terms.join(" AND "));
    }
    out.push(terms.join(" OR "));
    out.push(
        terms
            .iter()
            .map(|t| format!("{t}*"))
            .collect::<Vec<_>>()
            .join(" OR "),
    );
    out
}

pub fn search(
    conn: &Connection,
    query: &str,
    agent: Option<&str>,
    conversation_only: bool,
    newest_first: bool,
    limit: usize,
) -> Result<Vec<Hit>> {
    let limit = limit.clamp(1, 100);
    let mut last_error = None;
    let mut empty_result: Option<Vec<Hit>> = None;
    for fts_query in attempts(query) {
        let scoped = if conversation_only {
            format!("{{user_text assistant_text}}: ({fts_query})")
        } else {
            fts_query
        };
        let sql = format!(
            r#"SELECT s.agent, s.session_id, s.title, s.cwd, s.updated_at, s.resume_command,
                      snippet(messages_fts, -1, '[', ']', '…', 12) AS snip,
                      bm25(messages_fts, 1.0, 1.0, 0.35, 0.5) AS rank
               FROM messages_fts f
               JOIN messages m ON m.id = f.rowid
               JOIN sessions s ON s.id = m.session_row_id
               WHERE messages_fts MATCH ?1 AND m.role != 'tool'
                 AND (?2 IS NULL OR s.agent = ?2)
               ORDER BY {}
               LIMIT ?3"#,
            if newest_first {
                "s.updated_at DESC, rank"
            } else {
                "rank ASC"
            }
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        let rows = stmt.query_map(params![scoped, agent, (limit * 8) as i64], |row| {
            Ok(Hit {
                agent: row.get(0)?,
                session_id: row.get(1)?,
                title: row.get(2)?,
                cwd: row.get(3)?,
                updated_at: row.get(4)?,
                resume_command: row.get(5)?,
                snippet: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                score: -row.get::<_, f64>(7)?,
            })
        });
        let mut hits = Vec::new();
        let mut seen = HashSet::new();
        match rows {
            Ok(rows) => {
                for hit in rows {
                    let hit = hit?;
                    if seen.insert(hit.session_id.clone()) {
                        hits.push(hit);
                        if hits.len() >= limit {
                            break;
                        }
                    }
                }
                if hits.is_empty() {
                    empty_result = Some(hits);
                    continue;
                }
                return Ok(hits);
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }
    if let Some(hits) = empty_result {
        return Ok(hits);
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("search failed")))
}

#[derive(Debug)]
pub struct Transcript {
    pub title: String,
    pub cwd: Option<String>,
    pub messages: usize,
    pub text: String,
}

/// Rebuild a session transcript from the index for agent-to-agent handoff.
/// `focused` keeps only the tail (the stopping point), `full` keeps everything.
pub fn transcript(
    conn: &Connection,
    agent: &str,
    session_id: &str,
    focused: bool,
) -> Result<Transcript> {
    let (row_id, title, cwd): (i64, String, Option<String>) = conn
        .query_row(
            "SELECT id, title, cwd FROM sessions WHERE agent = ?1 AND session_id = ?2
             ORDER BY updated_at DESC, id DESC LIMIT 1",
            params![agent, session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .with_context(|| format!("session {agent}/{session_id} is not indexed yet"))?;

    let limit = if focused { 40i64 } else { -1 };
    let mut stmt = conn.prepare(
        "SELECT m.role,
                COALESCE(NULLIF(f.user_text, ''), NULLIF(f.assistant_text, ''),
                         NULLIF(f.tool_text, ''), '')
         FROM messages m JOIN messages_fts f ON f.rowid = m.id
         WHERE m.session_row_id = ?1 ORDER BY m.id DESC LIMIT ?2",
    )?;
    let mut rows: Vec<(String, String)> = stmt
        .query_map(params![row_id, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(Result::ok)
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();
    rows.reverse();

    let mut text = String::new();
    for (role, body) in &rows {
        let clipped: String = body.trim().chars().take(6_000).collect();
        text.push_str(&format!("### {role}\n\n{clipped}\n\n"));
        if text.len() > 160_000 {
            text.push_str("_(truncated)_\n");
            break;
        }
    }

    Ok(Transcript {
        title,
        cwd,
        messages: rows.len(),
        text,
    })
}

pub fn status(conn: &Connection, db_path: &Path) -> Result<(i64, i64, u64)> {
    let sessions: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    let messages: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let bytes = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    Ok((sessions, messages, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn index_and_search_finds_session_with_resume_command() {
        let dir = temp_dir("history");
        let tx_dir = dir.join("transcripts").join("codex");
        fs::create_dir_all(&tx_dir).unwrap();
        let mut f = fs::File::create(tx_dir.join("abc-123.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"role":"user","text":"where is resolveTerminalPath defined?","ts":"2026-01-01T00:00:00Z"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"role":"assistant","text":"It is in terminal path resolution.","ts":"2026-01-01T00:00:01Z"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"role":"tool","text":"secret tool output","ts":"2026-01-01T00:00:02Z"}}"#
        )
        .unwrap();

        let db = dir.join("history.sqlite");
        let mut conn = open(&db).unwrap();
        let report = index_dir(&mut conn, &dir.join("transcripts"), None, "/repo").unwrap();
        assert_eq!(report.files, 1);
        assert_eq!(report.messages, 3);

        let hits = search(&conn, "resolveTerminalPath", None, false, false, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].agent, "codex");
        assert_eq!(hits[0].resume_command, "codex resume abc-123");
        assert!(hits[0].snippet.contains("[resolveTerminalPath]"));

        let tool_hits = search(&conn, "secret", None, false, false, 10).unwrap();
        assert_eq!(tool_hits.len(), 0, "tool rows are not search results");

        let tool_scope = search(&conn, "secret", None, true, false, 10).unwrap();
        assert_eq!(tool_scope.len(), 0, "conversation scope excludes tool rows");

        let scoped = search(&conn, "resolveTerminalPath", None, true, false, 10).unwrap();
        assert_eq!(
            scoped.len(),
            1,
            "user text stays searchable in conversation scope"
        );
    }

    #[test]
    fn trailing_punctuation_and_camel_case_still_match() {
        let dir = temp_dir("punct");
        let tx_dir = dir.join("transcripts");
        fs::create_dir_all(&tx_dir).unwrap();
        let mut f = fs::File::create(tx_dir.join("s2.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"role":"assistant","text":"It reattaches resolveTerminalPath."}}"#
        )
        .unwrap();
        let db = dir.join("history.sqlite");
        let mut conn = open(&db).unwrap();
        index_dir(&mut conn, &tx_dir, Some("claude"), "/repo").unwrap();

        let hits = search(&conn, "resolveTerminalPath", None, false, false, 10).unwrap();
        assert_eq!(hits.len(), 1, "prefix fallback ignores the trailing dot");

        let words = search(&conn, "resolve terminal", None, false, false, 10).unwrap();
        assert_eq!(words.len(), 1, "identifiers column supplies split words");
    }

    #[test]
    fn phrase_falls_back_to_terms() {
        let dir = temp_dir("fallback");
        let tx_dir = dir.join("transcripts");
        fs::create_dir_all(&tx_dir).unwrap();
        let mut f = fs::File::create(tx_dir.join("s1.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"role":"assistant","text":"blank restore path is handled here"}}"#
        )
        .unwrap();
        let db = dir.join("history.sqlite");
        let mut conn = open(&db).unwrap();
        index_dir(&mut conn, &tx_dir, Some("claude"), "/repo").unwrap();
        let hits = search(&conn, "blank restore", None, true, false, 10).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
