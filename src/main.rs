#![cfg_attr(windows, windows_subsystem = "windows")]

mod agent;
mod browser;
#[cfg(feature = "gui")]
mod gui;
mod handoff;
mod history;
#[cfg(feature = "gui")]
mod pty;
#[cfg(feature = "gui")]
mod selftest;
mod workspace;

use anyhow::{Context, Result, bail};
use std::env;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    env::var_os("OMNIX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn omnix_dir() -> PathBuf {
    root().join(".omnix")
}

const HELP: &str = r#"omnix - a light workbench for worktrees, agents, and browsers

  omnix history index [dir] [--agent A] [--cwd PATH]
  omnix history search <query...> [--agent A] [--conversation] [--newest] [--limit N] [--json]
  omnix history status

  omnix agent track --agent A --session ID [--cwd PATH] [--pane KEY] [--workspace PATH]
  omnix sleep [--workspace PATH]
  omnix resume [--workspace PATH] [--run]
  omnix status

  omnix handoff <name> --prompt TEXT [--agent A] [--base REF] [--run]

  omnix browser fetch <url> [--engine native|lightpanda] [--dump markdown|html|semantic_tree_text]
  omnix browser serve [--port 9222]
  omnix browser doctor

  omnix app                      open the desktop app (no arguments does the same)

Defaults: history dir .omnix/transcripts; agent codex; browser engine lightpanda.
Docs: README.md, docs/browser.md, docs/orca-evidence.md
"#;

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn positional(args: &[String], value_flags: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with("--") {
            if value_flags.contains(&arg.as_str()) {
                skip = true;
            }
            continue;
        }
        out.push(arg.clone());
    }
    out
}

fn cmd_history(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "index" => {
            let pos = positional(&args[1..], &["--agent", "--cwd"]);
            let dir = pos
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(|| omnix_dir().join("transcripts"));
            let agent = value(args, "--agent");
            let cwd = value(args, "--cwd").unwrap_or_else(|| root().to_string_lossy().to_string());
            let db = omnix_dir().join("history.sqlite");
            let mut conn = history::open(&db)?;
            let report = history::index_dir(&mut conn, &dir, agent.as_deref(), &cwd)?;
            println!(
                "indexed {} files, {} messages, {} malformed lines -> {}",
                report.files,
                report.messages,
                report.skipped_lines,
                db.display()
            );
        }
        "search" => {
            let pos = positional(
                &args[1..],
                &["--agent", "--limit", "--cwd", "--scope", "--sort"],
            );
            let query = pos.join(" ");
            if query.trim().is_empty() {
                bail!("usage: omnix history search <query...>");
            }
            let limit = value(args, "--limit")
                .map(|n| n.parse::<usize>())
                .transpose()
                .context("--limit must be a number")?
                .unwrap_or(10);
            let conversation = has(args, "--conversation")
                || value(args, "--scope").as_deref() == Some("conversation");
            let newest =
                has(args, "--newest") || value(args, "--sort").as_deref() == Some("newest");
            let db = omnix_dir().join("history.sqlite");
            let conn = history::open(&db)?;
            let hits = history::search(
                &conn,
                &query,
                value(args, "--agent").as_deref(),
                conversation,
                newest,
                limit,
            )?;
            if has(args, "--json") {
                let rows: Vec<_> = hits
                    .iter()
                    .map(|h| {
                        serde_json::json!({
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
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if hits.is_empty() {
                println!("no hits");
            } else {
                for hit in hits {
                    println!(
                        "\n{}  {}  score {:.3}\n  {}\n  {}",
                        hit.agent,
                        hit.session_id,
                        hit.score,
                        hit.title,
                        hit.cwd.unwrap_or_default()
                    );
                    println!("  {}", hit.snippet.replace('\n', " "));
                    println!("  resume: {}", hit.resume_command);
                }
            }
        }
        "status" => {
            let db = omnix_dir().join("history.sqlite");
            let conn = history::open(&db)?;
            let (sessions, messages, bytes) = history::status(&conn, &db)?;
            println!(
                "sessions {sessions}  messages {messages}  db {:.1} MiB  {}",
                bytes as f64 / 1_048_576.0,
                db.display()
            );
        }
        _ => bail!("usage: omnix history index|search|status"),
    }
    Ok(())
}

fn cmd_agent(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("track") => {
            let agent = value(args, "--agent").context("--agent is required")?;
            let session = value(args, "--session").context("--session is required")?;
            let cwd = value(args, "--cwd").unwrap_or_else(|| root().to_string_lossy().to_string());
            let pane = value(args, "--pane").unwrap_or_else(|| "pane:1".into());
            let ws = value(args, "--workspace")
                .map(PathBuf::from)
                .unwrap_or_else(root);
            let record = workspace::track(&root(), &ws, &agent, &session, Path::new(&cwd), &pane)?;
            println!(
                "tracked {} {} in {} ({})",
                record.agent, record.session_id, record.cwd, record.pane_key
            );
            println!("resume: {}", record.resume_command);
        }
        _ => bail!("usage: omnix agent track --agent A --session ID"),
    }
    Ok(())
}

fn cmd_sleep(args: &[String]) -> Result<()> {
    let ws = value(args, "--workspace")
        .map(PathBuf::from)
        .unwrap_or_else(root);
    let state = workspace::sleep(&root(), &ws)?;
    println!(
        "sleeping {} with {} agent session(s)",
        state.path,
        state.sessions.len()
    );
    Ok(())
}

fn cmd_resume(args: &[String]) -> Result<()> {
    let ws = value(args, "--workspace")
        .map(PathBuf::from)
        .unwrap_or_else(root);
    let state = workspace::wake(&root(), &ws, has(args, "--run"))?;
    workspace::describe(&state);
    println!("awake {}", state.path);
    Ok(())
}

fn cmd_status(_args: &[String]) -> Result<()> {
    let workspaces = workspace::list(&root())?;
    if workspaces.is_empty() {
        println!("no workspaces tracked");
        return Ok(());
    }
    for ws in workspaces {
        println!(
            "{}{}  {}  ({} sessions)",
            if ws.sleeping { "zZ  " } else { "    " },
            ws.path,
            ws.branch.unwrap_or_default(),
            ws.sessions.len()
        );
    }
    Ok(())
}

fn cmd_handoff(args: &[String]) -> Result<()> {
    let pos = positional(args, &["--agent", "--prompt", "--base", "--name"]);
    let name = value(args, "--name")
        .or_else(|| pos.first().cloned())
        .context("handoff needs a name")?;
    let prompt = value(args, "--prompt").context("--prompt is required")?;
    let agent = value(args, "--agent").unwrap_or_else(|| "codex".into());
    let base = value(args, "--base");
    let (record, record_path) = handoff::create(
        &root(),
        &name,
        &agent,
        &prompt,
        base.as_deref(),
        has(args, "--run"),
    )?;
    println!("outcome: {}", record.outcome);
    println!("worktree: {}", record.worktree);
    println!("branch:   {}", record.branch);
    println!("agent:    {}", record.agent);
    println!("brief:    {}", record.brief);
    println!("record:   {}", record_path.display());
    if record.outcome == "not-delivered" {
        println!("launch:   cd {} && {}", record.worktree, record.agent);
    }
    if let Some(warning) = record.warning {
        println!("warning:  {warning}");
    }
    Ok(())
}

fn cmd_browser(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("fetch") => {
            let pos = positional(&args[1..], &["--engine", "--dump"]);
            let url = pos.first().context("usage: omnix browser fetch <url>")?;
            let engine = value(args, "--engine").unwrap_or_else(|| "lightpanda".into());
            let dump = value(args, "--dump").unwrap_or_else(|| "markdown".into());
            if !matches!(dump.as_str(), "markdown" | "html" | "semantic_tree_text") {
                bail!("--dump must be markdown, html, or semantic_tree_text");
            }
            browser::fetch(url, browser::Engine::parse(&engine)?, &dump)?;
        }
        Some("serve") => {
            let port = value(args, "--port")
                .map(|p| p.parse::<u16>())
                .transpose()
                .context("--port must be a number")?
                .unwrap_or(9222);
            browser::serve(port)?;
        }
        Some("doctor") => browser::doctor()?,
        _ => bail!("usage: omnix browser fetch|serve|doctor"),
    }
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("history") => cmd_history(&args[1..]),
        Some("agent") => cmd_agent(&args[1..]),
        Some("sleep") => cmd_sleep(&args[1..]),
        Some("resume") => cmd_resume(&args[1..]),
        Some("status") => cmd_status(&args[1..]),
        Some("handoff") => cmd_handoff(&args[1..]),
        Some("browser") => cmd_browser(&args[1..]),
        Some("app") => start_app(),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        None => start_app(),
        Some(other) => bail!("unknown command '{other}'. Try: omnix --help"),
    }
}

/// No arguments opens the app on desktop builds; keep the CLI as the fallback.
fn start_app() -> Result<()> {
    #[cfg(feature = "gui")]
    {
        return gui::run();
    }
    #[cfg(not(feature = "gui"))]
    {
        print!("{HELP}");
        Ok(())
    }
}

/// A GUI-subsystem binary has no console; attach to the caller's one so CLI output still works.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    // ATTACH_PARENT_PROCESS = (DWORD)-1
    unsafe { AttachConsole(u32::MAX) };
}

#[cfg(not(windows))]
fn attach_console() {}

fn main() {
    attach_console();
    if let Err(e) = run() {
        eprintln!("omnix: {e:#}");
        std::process::exit(1);
    }
}
