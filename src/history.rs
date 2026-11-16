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
struct TranscriptLine {
    role: String,
    text: String,
    #[serde(default)]
    ts: Option<String>,
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
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT OR REPLACE INTO meta(key,value) VALUES('schema_version', ?1)",
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

        let title = lines
            .iter()
            .find(|l| l.role == "user")
            .map(|l| collapse(&l.text, 80))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| file_name.to_string());
        let resume_command = super::agent::resume_command(&agent, file_name).unwrap_or_default();
        let updated_at = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let hash = content_hash(&lines);

        let tx = conn.transaction()?;
        let old_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM sessions WHERE file_path = ?1",
                params![path.to_string_lossy()],
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
                agent,
                file_name,
                path.to_string_lossy(),
                title,
                cwd,
                updated_at,
                lines.len() as i64,
                resume_command,
                hash
            ],
        )?;
        let session_row: i64 = tx.query_row(
            "SELECT id FROM sessions WHERE file_path = ?1",
            params![path.to_string_lossy()],
            |r| r.get(0),
        )?;

        let mut insert_msg = tx.prepare(
            "INSERT INTO messages(session_row_id, role, ts, text_truncated) VALUES(?1,?2,?3,?4)",
        )?;
        let mut insert_fts = tx.prepare(
            "INSERT INTO messages_fts(rowid,user_text,assistant_text,tool_text,identifiers) VALUES(?1,?2,?3,?4,?5)",
        )?;
        for line in &lines {
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
            report.messages += 1;
        }
        drop(insert_msg);
        drop(insert_fts);
        tx.commit()?;
        report.files += 1;
    }
    Ok(report)
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
            "perch-test-{tag}-{}-{}",
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
