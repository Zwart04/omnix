#![cfg_attr(windows, windows_subsystem = "windows")]

mod agent;
mod browser;
mod control;
mod dev;
#[cfg(feature = "gui")]
mod cookies;
#[cfg(feature = "gui")]
mod gc;
#[cfg(feature = "gui")]
mod gui;
mod handoff;
mod history;
mod locks;
mod mcp_proxy;
mod plugin;
mod preview;
mod procs;
mod projects;
#[cfg(feature = "gui")]
mod pty;
#[cfg(feature = "gui")]
mod selftest;
#[cfg(feature = "gui")]
mod shot;
mod setup;
mod sources;
mod update;
mod usage;
mod workspace;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::env;
use std::path::{Path, PathBuf};

pub(crate) fn root() -> PathBuf {
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
  omnix history scan             index every local agent transcript
  omnix history compact          vacuum the index after deleting transcripts
  omnix history status

  omnix agent track --agent A --session ID [--cwd PATH] [--pane KEY] [--workspace PATH]
  omnix agent status [--handoff NAME] [--json]
  omnix agent wait <name|pane> [--until turn-done,needs-approval,exited] [--timeout SEC] [--json]
  omnix agent send <name|pane> --text TEXT|--file FILE [--force]
  omnix sleep [--workspace PATH]
  omnix resume [--workspace PATH] [--run]
  omnix status

  omnix handoff <name> --repo PATH --prompt-file FILE [--prompt TEXT] [--agent A]
      [--base REF] [--worktree-dir PATH] [--link NAME=SRC] [--model M] [--effort E]
      [--mode plan|edit|full] [--role FILE|NAME] [--parent PANE] [--paths GLOB,...]
      [--max-parallel N] [--force] [--run]
  omnix handoff report <name> --status done|blocked|failed --file FILE|--text TEXT
  omnix handoff list [--open] [--json]
  omnix handoff close <name>

  omnix lock acquire <resource> [--owner NAME] [--ttl SEC] [--meta K=V]... [--json]
  omnix lock list [--json] [--prune]
  omnix lock release <resource|id> [--owner NAME] [--force]

  omnix usage [--json]           quota for codex, antigravity, claude
  omnix doctor [--json]          which agent CLIs Omnix can call
  omnix events [--limit N] [--kind K] [--json]   recent worker events
  omnix gc [--dry-run]           clean merged worktrees, old briefs, idle browser

  omnix browser fetch <url> [--engine native|lightpanda] [--dump markdown|html|semantic_tree_text]
  omnix browser serve [--port 9222]
  omnix browser stop [--session ID] [--all]
  omnix browser sessions [--json]
  omnix browser mcp [--port 9223] [--allow-eval] [--args...]
  omnix browser screenshot <url> --out FILE [--preset NAME|--width W --height H]
      [--mobile] [--dark] [--full-page]
  omnix browser session import --from native --domain HOST [--out FILE] [--yes]
  omnix browser session clear --domain HOST [--out FILE]
  omnix browser doctor

  omnix preview start <name> [--workspace PATH] [--keep]
  omnix preview logs <name> [--errors] [--workspace PATH]
  omnix preview stop <name> [--workspace PATH]
  omnix preview list [--json] [--workspace PATH]

  omnix plugin list [--json]
  omnix plugin search [query] [--json]
  omnix plugin install <catalog-name|folder|archive.tar.gz>
  omnix plugin update [name...]        omnix plugin remove <name>
  omnix plugin enable|disable <name>   omnix plugin doctor [--json]
  omnix plugin permissions <name> [--revoke read,network] [--grant read]
  omnix plugin new <name> --kind command|panel|hook|agent|browser-tool [--dir PATH]

  omnix dev task --prompt-file FILE --agent claude|codex|antigravity [--name N] [--no-run]
  omnix dev build [--workspace PATH] [--debug]
  omnix dev test [--workspace PATH] [--offline]
  omnix dev run [--build ID]            separate OMNIX_HOME
  omnix dev install [--build ID] --yes  only after the owner clicks install
  omnix dev restore --yes               bring back the previous binary
  omnix dev status [--json]

  omnix diagnostics memory [--json]

  omnix update [--check]         install the latest GitHub release

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

fn values(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = args;
    while let Some(index) = rest.iter().position(|arg| arg == flag) {
        if let Some(value) = rest.get(index + 1) {
            out.push(value.clone());
        }
        rest = &rest[index + 1..];
    }
    out
}

fn csv(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
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
        "scan" => {
            let db = omnix_dir().join("history.sqlite");
            let mut conn = history::open(&db)?;
            for (agent, path) in sources::roots(&root()) {
                println!("source {agent}: {}", path.display());
            }
            let report = sources::scan(&mut conn, &root())?;
            println!(
                "scanned {} files, {} messages ({} unchanged, {} unreadable, {} pruned)",
                report.files, report.messages, report.unchanged, report.skipped, report.pruned
            );
            for count in history::agent_counts(&conn)? {
                println!(
                    "  {:<10} {:>4} sessions {:>7} messages",
                    count.agent, count.sessions, count.messages
                );
            }
            println!("db {}", db.display());
        }
        "compact" => {
            let db = omnix_dir().join("history.sqlite");
            if !db.exists() {
                println!("no index at {}", db.display());
                return Ok(());
            }
            let before = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
            let conn = history::open(&db)?;
            history::compact(&conn)?;
            let after = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
            println!(
                "compacted {:.1} MiB -> {:.1} MiB  {}",
                before as f64 / 1_048_576.0,
                after as f64 / 1_048_576.0,
                db.display()
            );
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
        _ => bail!("usage: omnix history index|scan|search|compact|status"),
    }
    Ok(())
}

fn panes() -> Vec<Value> {
    control::read_panes(&control::runtime_dir())
}

fn pane_state(pane: &Value) -> String {
    pane.get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn filter_panes(panes: &[Value], handoff: Option<&str>, workspace: Option<&str>) -> Vec<Value> {
    panes
        .iter()
        .filter(|pane| {
            if let Some(name) = handoff {
                return pane.get("handoff").and_then(Value::as_str) == Some(name);
            }
            if let Some(path) = workspace {
                return pane
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(|cwd| cwd.starts_with(path))
                    .unwrap_or(false);
            }
            true
        })
        .cloned()
        .collect()
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
        Some("status") => {
            let rows = filter_panes(
                &panes(),
                value(args, "--handoff").as_deref(),
                value(args, "--workspace").as_deref(),
            );
            if has(args, "--json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "panes": rows }))?
                );
            } else if rows.is_empty() {
                println!("no panes (is the Omnix app running?)");
            } else {
                for pane in rows {
                    println!(
                        "{:<10} {:<16} {:<14} {}",
                        pane.get("paneKey").and_then(Value::as_str).unwrap_or("?"),
                        pane_state(&pane),
                        pane.get("handoff")
                            .and_then(Value::as_str)
                            .or_else(|| pane.get("label").and_then(Value::as_str))
                            .unwrap_or("-"),
                        pane.get("updatedAtLocal")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                    );
                }
            }
        }
        Some("wait") => {
            let pos = positional(&args[1..], &["--until", "--timeout", "--name"]);
            let target = value(args, "--name")
                .or_else(|| pos.first().cloned())
                .context("usage: omnix agent wait <name|pane>")?;
            let until = csv(
                &value(args, "--until")
                    .unwrap_or_else(|| "turn-done".into())
                    .split(',')
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            );
            let timeout = value(args, "--timeout")
                .map(|text| text.parse::<u64>())
                .transpose()
                .context("--timeout must be seconds")?
                .unwrap_or(600);
            if control::read_control(&control::runtime_dir()).is_none() {
                bail!("Omnix app is not running; open the app, then retry");
            }
            let started = std::time::Instant::now();
            loop {
                let rows = panes();
                if let Some(pane) = control::find_pane(&rows, &target) {
                    let state = pane_state(pane);
                    let hit = until.iter().any(|want| want == &state)
                        || (state == "exited" && until.iter().any(|want| want == "turn-done"));
                    if hit {
                        if has(args, "--json") {
                            println!("{}", serde_json::to_string_pretty(pane)?);
                        } else {
                            println!(
                                "{} {}",
                                pane.get("paneKey").and_then(Value::as_str).unwrap_or("?"),
                                state
                            );
                        }
                        return Ok(());
                    }
                }
                if timeout > 0 && started.elapsed().as_secs() >= timeout {
                    bail!(
                        "timed out after {timeout}s waiting for '{}' ({})",
                        target,
                        until.join("|")
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        Some("send") => {
            let pos = positional(&args[1..], &["--text", "--file", "--name"]);
            let target = value(args, "--name")
                .or_else(|| pos.first().cloned())
                .context("usage: omnix agent send <name|pane> --text TEXT|--file FILE")?;
            let text = match (value(args, "--text"), value(args, "--file")) {
                (Some(text), None) => text,
                (None, Some(path)) => {
                    let meta = std::fs::metadata(&path).with_context(|| format!("read {path}"))?;
                    if meta.len() > 1024 * 1024 {
                        bail!("--file is too large (limit 1 MiB)");
                    }
                    std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?
                }
                _ => bail!("use exactly one of --text or --file"),
            };
            if text.is_empty() {
                bail!("message is empty");
            }
            if !has(args, "--force") {
                if let Some(pattern) = control::held(&text) {
                    bail!("message contains a held command ('{pattern}'); pass --force");
                }
            }
            let reply = control::request(
                &control::runtime_dir(),
                json!({
                    "cmd": "send",
                    "target": target,
                    "text": text,
                    "force": has(args, "--force"),
                }),
            )?;
            println!(
                "sent to {} ({})",
                reply.get("paneKey").and_then(Value::as_str).unwrap_or("?"),
                reply.get("state").and_then(Value::as_str).unwrap_or("unknown")
            );
        }
        _ => bail!("usage: omnix agent track|status|wait|send"),
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
    if !has(args, "--keep") {
        let stopped = preview::stop_all(Path::new(&state.path), false);
        if !stopped.is_empty() {
            println!("stopped preview(s): {}", stopped.join(", "));
        }
    }
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

fn handoff_repo(args: &[String]) -> Result<PathBuf> {
    let base = value(args, "--repo")
        .map(PathBuf::from)
        .unwrap_or_else(root);
    handoff::main_repo(&base)
}

fn quota_block(agent: &str, home: &Path, stop: f64) -> Option<String> {
    let snapshot = usage::snapshot_for(&root(), home);
    let entry = snapshot
        .get("agents")
        .and_then(Value::as_array)?
        .iter()
        .find(|entry| entry.get("agent").and_then(Value::as_str) == Some(agent))?;
    let used = entry
        .get("windows")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|window| window.get("usedPercent").and_then(Value::as_f64))
        .fold(0.0f64, f64::max);
    if used >= stop {
        Some(format!("{agent} quota is {used:.0}% used (limit {stop:.0}%)"))
    } else {
        None
    }
}

/// Cheap, invoked-only gate: read live panes from the app, then decide whether
/// to spawn now or queue the handoff. No background runner.
fn run_gate(agent: &str, max_parallel: usize) -> Result<Option<String>> {
    let state = control::request(&control::runtime_dir(), json!({"cmd": "state"}))?;
    let active = state
        .get("panes")
        .and_then(Value::as_array)
        .map(|panes| {
            panes
                .iter()
                .filter(|pane| {
                    matches!(
                        pane.get("state").and_then(Value::as_str),
                        Some("working" | "needs-approval")
                    )
                })
                .count()
        })
        .unwrap_or(0);
    if active >= max_parallel {
        return Ok(Some(format!(
            "{active} worker(s) active (max {max_parallel})"
        )));
    }
    if state
        .pointer("/memory/over")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(Some("AI memory budget is full".into()));
    }
    let stop = env::var("OMNIX_QUOTA_STOP")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(90.0);
    Ok(quota_block(agent, &control::home(), stop))
}

fn cmd_handoff(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("list") => {
            let open_only = has(args, "--open");
            let records: Vec<handoff::Handoff> = handoff::list_all()
                .into_iter()
                .filter(|record| !open_only || !record.closed)
                .collect();
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&records)?);
            } else if records.is_empty() {
                println!("no handoffs");
            } else {
                for record in records {
                    println!(
                        "{:<18} {:<18} {:<10} {:<10} {}",
                        record.name,
                        record.outcome,
                        record.agent,
                        record.status.unwrap_or_else(|| "-".into()),
                        record.paths.join(",")
                    );
                    println!("  worktree {}", record.worktree);
                    if let Some(report) = record.report {
                        println!("  report   {report}");
                    }
                    if let Some(reason) = record.queued_reason {
                        println!("  queued   {reason}");
                    }
                }
            }
        }
        Some("report") => {
            let pos = positional(&args[1..], &["--repo", "--status", "--file", "--text", "--name"]);
            let name = value(args, "--name")
                .or_else(|| pos.first().cloned())
                .context("usage: omnix handoff report <name> --status S --file F")?;
            let status = value(args, "--status").context("--status is required")?;
            let text = match (value(args, "--file"), value(args, "--text")) {
                (Some(path), None) => {
                    let meta = std::fs::metadata(&path).with_context(|| format!("read {path}"))?;
                    if meta.len() > 1024 * 1024 {
                        bail!("--file is too large (limit 1 MiB)");
                    }
                    std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?
                }
                (None, Some(text)) => text,
                _ => bail!("use exactly one of --file or --text"),
            };
            let repo = match value(args, "--repo") {
                Some(path) => handoff::main_repo(&PathBuf::from(path))?,
                None => PathBuf::from(
                    handoff::find_any(&name)
                        .with_context(|| format!("no handoff '{name}'"))?
                        .repo,
                ),
            };
            let record = handoff::report(&repo, &name, &status, &text)?;
            control::notify("Handoff reported", &format!("{} {}", record.name, status));
            if let Some(path) = record.report {
                println!("reported {} {} -> {path}", record.name, status);
            }
        }
        Some("close") => {
            let pos = positional(&args[1..], &["--repo", "--name"]);
            let name = value(args, "--name")
                .or_else(|| pos.first().cloned())
                .context("usage: omnix handoff close <name>")?;
            let record = match handoff_repo(args) {
                Ok(repo) => handoff::close_in(&repo, &name)?,
                Err(_) => handoff::close(&name)?,
            };
            println!("closed {} ({})", record.name, record.worktree);
        }
        _ => {
            let pos = positional(
                args,
                &[
                    "--agent",
                    "--prompt",
                    "--prompt-file",
                    "--base",
                    "--name",
                    "--repo",
                    "--worktree-dir",
                    "--link",
                    "--model",
                    "--effort",
                    "--mode",
                    "--role",
                    "--parent",
                    "--paths",
                    "--max-parallel",
                ],
            );
            let name = value(args, "--name")
                .or_else(|| pos.first().cloned())
                .context("handoff needs a name")?;
            let links = values(args, "--link")
                .into_iter()
                .map(|pair| {
                    let (key, path) = pair
                        .split_once('=')
                        .with_context(|| format!("--link must be NAME=SRC, got '{pair}'"))?;
                    Ok((key.to_string(), PathBuf::from(path)))
                })
                .collect::<Result<Vec<_>>>()?;
            let options = handoff::CreateOptions {
                name: name.clone(),
                agent: value(args, "--agent").unwrap_or_else(|| "codex".into()),
                prompt: value(args, "--prompt").unwrap_or_default(),
                prompt_file: value(args, "--prompt-file").map(PathBuf::from),
                repo: value(args, "--repo").map(PathBuf::from),
                base: value(args, "--base"),
                worktree_dir: value(args, "--worktree-dir").map(PathBuf::from),
                links,
                role: value(args, "--role"),
                parent: value(args, "--parent"),
                model: value(args, "--model"),
                effort: value(args, "--effort"),
                mode: value(args, "--mode"),
                paths: csv(&values(args, "--paths")),
            };
            let (mut record, record_path) = handoff::create_with(&root(), options)?;
            handoff::remember(&record_path)?;
            if has(args, "--run") {
                let reason = if has(args, "--force") {
                    None
                } else {
                    let max_parallel = value(args, "--max-parallel")
                        .map(|text| text.parse::<usize>())
                        .transpose()
                        .context("--max-parallel must be a number")?
                        .unwrap_or(2);
                    run_gate(&record.agent, max_parallel)?
                };
                if let Some(reason) = reason {
                    handoff::mark_queued(&mut record, &reason)?;
                    println!("outcome: queued");
                    println!("reason:  {reason}");
                } else {
                    let reply = control::request(
                        &control::runtime_dir(),
                        json!({
                            "cmd": "spawnHandoff",
                            "record": record,
                        }),
                    )
                    .with_context(|| {
                        format!(
                            "handoff saved at {}; open the Omnix app to run it",
                            record_path.display()
                        )
                    })?;
                    let pane = reply
                        .get("paneKey")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    handoff::mark_delivered(&mut record, &pane)?;
                    println!("pane:    {pane}");
                }
            }
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
        }
    }
    Ok(())
}

fn cmd_lock(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let pos = positional(&args[1..], &["--owner", "--ttl", "--meta"]);
    match sub {
        "acquire" => {
            let resource = pos
                .first()
                .context("usage: omnix lock acquire <resource> [--owner NAME] [--ttl SEC] [--meta K=V]")?;
            let owner = value(args, "--owner").unwrap_or_else(locks::default_owner);
            let ttl = value(args, "--ttl")
                .map(|text| text.parse::<u64>())
                .transpose()
                .context("--ttl must be a number of seconds")?
                .unwrap_or(locks::DEFAULT_TTL);
            let mut meta = std::collections::BTreeMap::new();
            for pair in values(args, "--meta") {
                let (key, value) = pair
                    .split_once('=')
                    .with_context(|| format!("--meta must be KEY=VALUE, got '{pair}'"))?;
                if key.trim().is_empty() {
                    bail!("--meta key is empty");
                }
                meta.insert(key.trim().to_string(), value.trim().to_string());
            }
            let lock = locks::acquire_with(&root(), resource, &owner, ttl, meta)?;
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&lock)?);
            } else {
                println!("locked:  {}", lock.resource);
                println!("owner:   {}", lock.owner);
                println!("pid:     {}", lock.pid);
                println!("expires: {}", control::local_time(lock.expires_at));
                println!("id:      {}", lock.id);
            }
        }
        "list" => {
            if has(args, "--prune") {
                let removed = locks::prune(&root());
                if !has(args, "--json") {
                    println!("pruned {removed} stale lock(s)");
                }
            }
            let rows = locks::list(&root());
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if rows.is_empty() {
                println!("no active locks ({})", locks::locks_dir(&root()).display());
            } else {
                for lock in rows {
                    println!(
                        "{:<36} {:<24} pid {:<8} until {}",
                        lock.resource,
                        lock.owner,
                        lock.pid,
                        control::local_time(lock.expires_at)
                    );
                }
            }
        }
        "release" => {
            let target = pos
                .first()
                .context("usage: omnix lock release <resource|id> [--owner NAME] [--force]")?;
            let owner = value(args, "--owner").unwrap_or_else(locks::default_owner);
            match locks::release(&root(), target, Some(&owner), has(args, "--force"))? {
                Some(lock) => println!("released: {} (owner {})", lock.resource, lock.owner),
                None => println!("no lock named '{target}'"),
            }
        }
        _ => bail!("usage: omnix lock acquire|list|release"),
    }
    Ok(())
}

fn cmd_usage(args: &[String]) -> Result<()> {
    let mut snapshot = usage::snapshot_live_for(&root(), &control::home());
    if let Value::Object(map) = &mut snapshot {
        map.insert("at".into(), json!(control::epoch()));
        map.insert("atLocal".into(), json!(control::local_time(control::epoch())));
    }
    if has(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }
    let mut lines = Vec::new();
    for entry in snapshot
        .get("agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let agent = entry.get("agent").and_then(Value::as_str).unwrap_or("?");
        let used = entry
            .get("windows")
            .and_then(Value::as_array)
            .map(|windows| {
                windows
                    .iter()
                    .filter_map(|w| w.get("usedPercent").and_then(Value::as_f64))
                    .fold(0.0f64, f64::max)
            })
            .unwrap_or(0.0);
        lines.push(format!("{agent:<12} {used:.0}% used"));
    }
    if lines.is_empty() {
        lines.push("no quota data (codex/antigravity not logged in?)".into());
    }
    if let Some(ram) = snapshot.get("ram") {
        lines.push(format!(
            "ram          {} of {} used",
            bytes(ram.get("usedBytes").and_then(Value::as_u64).unwrap_or(0)),
            bytes(ram.get("totalBytes").and_then(Value::as_u64).unwrap_or(0))
        ));
    }
    lines.push(format!("at           {}", control::local_time(control::epoch())));
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

fn cmd_doctor(args: &[String]) -> Result<()> {
    let agents = ["claude", "codex", "antigravity"];
    let mut rows = Vec::new();
    for agent in agents {
        let path = crate::agent::resolve(agent);
        let version = crate::agent::version(agent);
        rows.push(json!({
            "agent": agent,
            "binary": crate::agent::launch_cmd(agent),
            "path": path.map(|p| p.to_string_lossy().into_owned()),
            "version": version,
        }));
    }
    let control = control::read_control(&control::runtime_dir());
    let payload = json!({
        "agents": rows,
        "omnix": {
            "version": env!("CARGO_PKG_VERSION"),
            "control": control.is_some(),
            "root": root().to_string_lossy(),
        },
        "lightpanda": browser::version(),
        "atLocal": control::local_time(control::epoch()),
    });
    if has(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    for row in payload["agents"].as_array().cloned().unwrap_or_default() {
        let agent = row.get("agent").and_then(Value::as_str).unwrap_or("?");
        match row.get("path").and_then(Value::as_str) {
            Some(path) => println!(
                "{agent:<12} {:<40} {}",
                path,
                row.get("version").and_then(Value::as_str).unwrap_or("installed")
            ),
            None => println!("{agent:<12} missing"),
        }
    }
    if crate::agent::resolve("claude").is_none() {
        println!();
        println!("Claude Code is not on PATH. Install it with the official installer:");
        println!("  npm install -g @anthropic-ai/claude-code");
        println!("  https://docs.anthropic.com/en/docs/claude-code/setup");
        println!("A desktop app's bundled binary moves on every update; do not link it.");
    }
    println!();
    println!(
        "Omnix {} · control socket {} · Lightpanda {}",
        env!("CARGO_PKG_VERSION"),
        if control.is_some() { "live" } else { "off" },
        browser::version().unwrap_or_else(|| "missing".into())
    );
    Ok(())
}

fn cmd_events(args: &[String]) -> Result<()> {
    let limit = value(args, "--limit")
        .map(|text| text.parse::<usize>())
        .transpose()
        .context("--limit must be a number")?
        .unwrap_or(20);
    let kinds = value(args, "--kind").map(|text| csv(&[text]));
    let raw = std::fs::read_to_string(control::events_path()).unwrap_or_default();
    let mut rows: Vec<Value> = raw
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|row: &Value| {
            kinds
                .as_ref()
                .map(|wanted| {
                    row.get("kind")
                        .and_then(Value::as_str)
                        .map(|kind| wanted.iter().any(|want| want == kind))
                        .unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .collect();
    let start = rows.len().saturating_sub(limit);
    rows.drain(..start);
    if has(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no events");
        return Ok(());
    }
    for row in rows {
        let at = row
            .get("atLocal")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let kind = row.get("kind").and_then(Value::as_str).unwrap_or("?");
        let who = row
            .get("handoff")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| row.get("pane").and_then(Value::as_str))
            .unwrap_or("");
        let extra = row
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| row.get("code").map(|_| "exit"))
            .unwrap_or("");
        println!("{at}  {kind:<22} {who:<18} {extra}");
    }
    Ok(())
}

fn cmd_gc(args: &[String]) -> Result<()> {
    #[cfg(feature = "gui")]
    {
        let preview = gc::run(&root(), has(args, "--dry-run"))?;
        if has(args, "--json") {
            println!("{}", serde_json::to_string_pretty(&preview)?);
        } else {
            for line in preview
                .get("actions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                println!("{}", line.as_str().unwrap_or(""));
            }
            if has(args, "--dry-run") {
                println!("dry run: nothing changed");
            }
        }
        return Ok(());
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = args;
        bail!("gc needs a build with the gui feature")
    }
}


fn cmd_preview(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let ws = value(args, "--workspace")
        .map(PathBuf::from)
        .unwrap_or_else(root);
    match sub {
        "start" => {
            let pos = positional(&args[1..], &["--workspace"]);
            let name = pos.first().context("usage: omnix preview start <name>")?;
            let value = preview::start(&ws, name, has(args, "--keep"))?;
            if value.get("alreadyRunning").and_then(Value::as_bool) == Some(true) {
                println!(
                    "preview '{}' already running: {} (pid {})",
                    value["name"].as_str().unwrap_or(name),
                    value["url"].as_str().unwrap_or(""),
                    value["pid"].as_u64().unwrap_or(0)
                );
            } else {
                println!(
                    "preview '{}' ready: {} (pid {})
log: {}",
                    value["name"].as_str().unwrap_or(name),
                    value["url"].as_str().unwrap_or(""),
                    value["pid"].as_u64().unwrap_or(0),
                    value["log"].as_str().unwrap_or("")
                );
            }
        }
        "logs" => {
            let pos = positional(&args[1..], &["--workspace"]);
            let name = pos.first().context("usage: omnix preview logs <name>")?;
            println!("{}", preview::logs(&ws, name, has(args, "--errors"))?);
        }
        "stop" => {
            let pos = positional(&args[1..], &["--workspace"]);
            let name = pos.first().context("usage: omnix preview stop <name>")?;
            let value = preview::stop(&ws, name)?;
            println!(
                "stopped preview '{}' (pid {})",
                value["name"].as_str().unwrap_or(name),
                value["pid"].as_u64().unwrap_or(0)
            );
        }
        "list" => {
            let rows = preview::list(&ws);
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if rows.is_empty() {
                println!("no previews in {}", ws.display());
            } else {
                for row in rows {
                    println!(
                        "{:<20} {:<24} pid {:<8} {}{}",
                        row["name"].as_str().unwrap_or("?"),
                        row["url"].as_str().unwrap_or(""),
                        row["pid"].as_u64().unwrap_or(0),
                        if row["alive"].as_bool().unwrap_or(false) {
                            "alive"
                        } else {
                            "dead"
                        },
                        if row["ready"].as_bool().unwrap_or(false) {
                            ""
                        } else {
                            " (port closed)"
                        }
                    );
                }
            }
        }
        _ => bail!("usage: omnix preview start|logs|stop|list"),
    }
    Ok(())
}

fn cmd_plugin(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let pos = positional(
        &args[1..],
        &["--kind", "--dir", "--sha256", "--revoke", "--grant", "--limit"],
    );
    match sub {
        "list" => {
            let rows = plugin::list();
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if rows.is_empty() {
                println!("no plugins installed ({})", plugin::plugins_dir().display());
            } else {
                for row in rows {
                    println!(
                        "{:<24} {:<10} {:<8} {:<18} {}",
                        row["name"].as_str().unwrap_or("?"),
                        row["version"].as_str().unwrap_or("-"),
                        if row["enabled"].as_bool().unwrap_or(false) {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        row["kinds"].as_array().map(|k| {
                            k.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")
                        }).unwrap_or_default(),
                        row["permissions"].as_str().unwrap_or("")
                    );
                    if let Some(error) = row["error"].as_str() {
                        println!("  error: {error}");
                    }
                }
            }
        }
        "search" => {
            let query = pos.join(" ");
            let value = plugin::search(&query)?;
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else if value["plugins"].as_array().map(Vec::is_empty).unwrap_or(true) {
                println!("no catalog plugin matches '{}'", query.trim());
            } else {
                for row in value["plugins"].as_array().cloned().unwrap_or_default() {
                    println!(
                        "{:<24} {:<10} {}{}",
                        row["name"].as_str().unwrap_or("?"),
                        row["version"].as_str().unwrap_or("?"),
                        row["description"].as_str().unwrap_or(""),
                        row["installed"]
                            .as_str()
                            .map(|v| format!("  (installed {v})"))
                            .unwrap_or_default()
                    );
                    if let Some(perms) = row["permissions"].as_str().filter(|p| *p != "none") {
                        println!("  permissions: {perms}");
                    }
                }
            }
        }
        "install" => {
            let source = pos.first().context("usage: omnix plugin install <name|folder|archive>")?;
            let value = plugin::install(source)?;
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!(
                    "installed {} {} -> {}",
                    value["name"].as_str().unwrap_or("?"),
                    value["version"].as_str().unwrap_or("?"),
                    value["path"].as_str().unwrap_or("")
                );
                println!("permissions: {}", value["permissions"].as_str().unwrap_or("none"));
                println!("{}", value["note"].as_str().unwrap_or(""));
            }
        }
        "update" => {
            let value = plugin::update(&pos)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        "remove" => {
            let name = pos.first().context("usage: omnix plugin remove <name>")?;
            let value = plugin::remove(name)?;
            println!("removed {}", value["name"].as_str().unwrap_or(name));
        }
        "enable" | "disable" => {
            let name = pos.first().context("usage: omnix plugin enable|disable <name>")?;
            let value = plugin::set_enabled(name, sub == "enable")?;
            println!(
                "{} {}",
                value["name"].as_str().unwrap_or(name),
                if sub == "enable" { "enabled" } else { "disabled" }
            );
        }
        "doctor" => {
            let value = plugin::doctor();
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                if value["plugins"].as_array().map(Vec::is_empty).unwrap_or(true) {
                    println!("no plugins installed");
                }
                for row in value["plugins"].as_array().cloned().unwrap_or_default() {
                    println!(
                        "{} {}",
                        if row["ok"].as_bool().unwrap_or(false) { "ok  " } else { "FAIL" },
                        row["name"].as_str().unwrap_or("?")
                    );
                    for problem in row["problems"].as_array().cloned().unwrap_or_default() {
                        println!("  - {}", problem.as_str().unwrap_or("?"));
                    }
                }
            }
        }
        "permissions" => {
            let name = pos.first().context("usage: omnix plugin permissions <name>")?;
            if let Some(keys) = value(args, "--revoke") {
                let keys = csv(&[keys]);
                plugin::revoke(name, &keys)?;
            }
            if let Some(keys) = value(args, "--grant") {
                let keys = csv(&[keys]);
                plugin::grant(name, &keys)?;
            }
            let rows = plugin::list();
            if let Some(row) = rows.iter().find(|row| row["name"].as_str() == Some(name)) {
                println!("{}: {}", name, row["permissions"].as_str().unwrap_or("none"));
                println!(
                    "granted: {}",
                    row["granted"]
                        .as_array()
                        .map(|keys| keys.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","))
                        .unwrap_or_default()
                );
            } else {
                bail!("plugin '{name}' is not installed");
            }
        }
        "new" => {
            let name = pos.first().context("usage: omnix plugin new <name> --kind command")?;
            let kind = value(args, "--kind").unwrap_or_else(|| "command".into());
            let dir = value(args, "--dir").map(PathBuf::from);
            let value = plugin::scaffold(name, &kind, dir)?;
            println!(
                "created {} ({})",
                value["path"].as_str().unwrap_or("?"),
                value["kind"].as_str().unwrap_or("?")
            );
            println!("next: {}", value["next"].as_str().unwrap_or(""));
        }
        "run" => {
            let name = pos.first().context("usage: omnix plugin run <name> [args...]")?;
            let forwarded: Vec<String> = args.iter().skip(2).cloned().collect();
            let value = plugin::run_command(name, &forwarded)?;
            print_plugin_result(&value);
        }
        _ => bail!("usage: omnix plugin list|search|install|update|remove|enable|disable|doctor|permissions|new|run"),
    }
    Ok(())
}

fn print_plugin_result(value: &Value) {
    if let Some(text) = value.as_str() {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
        return;
    }
    if let Some(text) = value.get("stdout").and_then(Value::as_str) {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    }
    if let Some(text) = value.get("stderr").and_then(Value::as_str) {
        eprint!("{text}");
    }
    if value.get("stdout").is_none() && value.get("stderr").is_none() {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    }
    if let Some(code) = value.get("exitCode").and_then(Value::as_i64) {
        if code != 0 {
            std::process::exit(code as i32);
        }
    }
}

fn cmd_dev(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "task" => {
            let prompt_file = value(args, "--prompt-file").context("--prompt-file is required")?;
            let agent = value(args, "--agent").unwrap_or_else(|| "codex".into());
            let (record, path) = dev::task(dev::TaskOptions {
                prompt_file: PathBuf::from(prompt_file),
                agent,
                name: value(args, "--name"),
                repo: value(args, "--repo").map(PathBuf::from),
                run: !has(args, "--no-run"),
            })?;
            println!("outcome:  {}", record.outcome);
            println!("worktree: {}", record.worktree);
            println!("branch:   {}", record.branch);
            println!("agent:    {}", record.agent);
            println!("brief:    {}", record.brief);
            println!("record:   {}", path.display());
            println!("next:     finish the branch, run omnix dev build/test, open a PR");
        }
        "build" => {
            let ws = value(args, "--workspace").map(PathBuf::from);
            let build = dev::build(ws, !has(args, "--debug"))?;
            println!(
                "built {}  {}  {:.1} MiB  {}",
                build.id,
                build.commit,
                build.size as f64 / 1_048_576.0,
                build.binary
            );
        }
        "test" => {
            let value = dev::test(value(args, "--workspace").map(PathBuf::from), has(args, "--offline"))?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            if value["ok"].as_bool() == Some(false) {
                bail!("dev test failed");
            }
        }
        "run" => {
            let value = dev::run(value(args, "--build").as_deref())?;
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!(
                    "started {} (pid {}) with OMNIX_HOME={}",
                    value["build"].as_str().unwrap_or("?"),
                    value["pid"].as_u64().unwrap_or(0),
                    value["home"].as_str().unwrap_or("")
                );
            }
        }
        "install" => {
            if !has(args, "--yes") {
                bail!("refusing to replace the running binary without --yes; the app asks first");
            }
            let value = dev::install(value(args, "--build").as_deref(), has(args, "--restart"))?;
            println!("{}", value["message"].as_str().unwrap_or("installed"));
        }
        "restore" => {
            if !has(args, "--yes") {
                bail!("refusing to restore without --yes");
            }
            let value = dev::restore(has(args, "--restart"))?;
            println!("{}", value["message"].as_str().unwrap_or("restored"));
        }
        "status" | "list" => {
            let value = dev::status();
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                for build in value["builds"].as_array().cloned().unwrap_or_default() {
                    println!(
                        "{:<24} {:<10} {}  {}",
                        build["id"].as_str().unwrap_or("?"),
                        build["commit"].as_str().unwrap_or("?"),
                        build["branch"].as_str().unwrap_or(""),
                        build["binary"].as_str().unwrap_or("")
                    );
                }
                if let Some(installed) = value["installed"].get("build").and_then(Value::as_str) {
                    println!("installed: {installed}");
                }
            }
        }
        _ => bail!("usage: omnix dev task|build|test|run|install|restore|status"),
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
        Some("stop") => {
            let value = browser::stop(value(args, "--session").as_deref(), has(args, "--all"))?;
            if has(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else if value["stopped"].as_bool().unwrap_or(false) {
                if let Some(session) = value["session"].as_str() {
                    println!("closed browser session {session}");
                    if value["daemon"].as_u64().is_some() {
                        println!("shared Lightpanda daemon is still running (use --all to stop it)");
                    }
                } else {
                    println!("stopped shared lightpanda");
                }
            } else {
                println!("no browser session to close");
            }
        }
        Some("sessions") => {
            if has(args, "--json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&browser::session_list(&root()))?
                );
            } else {
                browser::print_sessions()?;
            }
        }
        Some("session") => {
            #[cfg(feature = "gui")]
            {
                let action = args.get(1).map(String::as_str).unwrap_or("");
                let from = value(args, "--from").unwrap_or_else(|| "native".into());
                if from != "native" {
                    bail!("browser session supports --from native only");
                }
                let domain = value(args, "--domain").context("--domain HOST is required")?;
                let out = value(args, "--out");
                let value = match action {
                    "import" => cookies::import(&root(), &domain, out.as_deref(), has(args, "--yes"))?,
                    "clear" => cookies::clear(&root(), &domain, out.as_deref())?,
                    _ => bail!("usage: omnix browser session import|clear --from native --domain HOST"),
                };
                if has(args, "--json") {
                    println!("{}", serde_json::to_string_pretty(&value)?);
                } else if action == "import" {
                    println!(
                        "copied {} cookie(s) for {} -> {}",
                        value["count"].as_u64().unwrap_or(0),
                        value["domain"].as_str().unwrap_or(""),
                        value["path"].as_str().unwrap_or("")
                    );
                    println!("agent: omnix browser mcp   (the session file is picked up automatically)");
                    println!("clear: omnix browser session clear --domain {}", value["domain"].as_str().unwrap_or(""));
                } else {
                    println!(
                        "{} cookie session for {}: {}",
                        if value["removed"].as_bool().unwrap_or(false) {
                            "removed"
                        } else {
                            "no"
                        },
                        value["domain"].as_str().unwrap_or(""),
                        value["path"].as_str().unwrap_or("")
                    );
                }
            }
            #[cfg(not(feature = "gui"))]
            {
                let _ = args;
                bail!("browser session needs a build with the gui feature");
            }
        }
        Some("screenshot") => {
            #[cfg(feature = "gui")]
            {
                let opts = shot::parse_args(&args[1..])?;
                let value = shot::capture(&opts)?;
                println!(
                    "saved {}  {}x{}",
                    value["path"].as_str().unwrap_or("?"),
                    value["width"].as_i64().unwrap_or(0),
                    value["height"].as_i64().unwrap_or(0)
                );
            }
            #[cfg(not(feature = "gui"))]
            {
                let _ = &args;
                bail!("browser screenshot needs a build with the gui feature");
            }
        }
        Some("mcp") => {
            let forwarded: Vec<String> = args[1..].to_vec();
            browser::mcp(&forwarded)?;
        }
        Some("doctor") => browser::doctor()?,
        _ => bail!("usage: omnix browser fetch|serve|stop|sessions|mcp|doctor"),
    }
    Ok(())
}

fn cmd_update(args: &[String]) -> Result<()> {
    if has(args, "--check") {
        let status = update::check_payload()?;
        if status["available"].as_bool().unwrap_or(false) {
            println!(
                "omnix {} -> {}",
                status["current"].as_str().unwrap_or("?"),
                status["version"].as_str().unwrap_or("?")
            );
        } else {
            println!(
                "omnix {} is up to date",
                status["current"].as_str().unwrap_or("?")
            );
        }
        return Ok(());
    }
    let result = update::install(false)?;
    println!(
        "{}",
        result["message"].as_str().unwrap_or("update finished")
    );
    Ok(())
}

fn cmd_diagnostics(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("memory") => {
            let json_out = has(args, "--json");
            let table = procs::snapshot();
            let pid = std::process::id();
            // Run from a terminal inside the app: report the GUI tree, not the
            // few megabytes this CLI owns.
            let app = table.ancestor_named(pid, "omnix").unwrap_or(pid);
            let (rss, processes) = table.sum_tree(app);
            let tree = table.descendants(app);
            if json_out {
                let top: Vec<Value> = tree
                    .iter()
                    .take(12)
                    .map(|p| {
                        json!({
                            "pid": p.pid,
                            "ppid": p.ppid,
                            "name": p.name,
                            "rss": p.rss,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "app": { "pid": app, "rss": rss, "processes": processes },
                        "host": usage::ram(),
                        "processes": top,
                    }))?
                );
            } else {
                println!("Omnix pid {app}: {processes} processes, {}", bytes(rss));
                if let Some(host) = usage::ram() {
                    let used = host.get("usedBytes").and_then(Value::as_u64).unwrap_or(0);
                    let total = host.get("totalBytes").and_then(Value::as_u64).unwrap_or(0);
                    println!("Host: {} of {} used", bytes(used), bytes(total));
                }
                for p in tree.iter().take(12) {
                    println!("{:>7}  {:>10}  {}", p.pid, bytes(p.rss), p.name);
                }
            }
        }
        _ => bail!("usage: omnix diagnostics memory [--json]"),
    }
    Ok(())
}

fn bytes(value: u64) -> String {
    if value >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024 * 1024 {
        format!("{:.0} MiB", value as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KiB", value as f64 / 1024.0)
    }
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
        Some("lock") => cmd_lock(&args[1..]),
        Some("usage") => cmd_usage(&args[1..]),
        Some("doctor") => cmd_doctor(&args[1..]),
        Some("events") => cmd_events(&args[1..]),
        Some("gc") => cmd_gc(&args[1..]),
        Some("preview") => cmd_preview(&args[1..]),
        Some("plugin") => cmd_plugin(&args[1..]),
        Some("dev") => cmd_dev(&args[1..]),
        Some("browser") => cmd_browser(&args[1..]),
        Some("diagnostics") => cmd_diagnostics(&args[1..]),
        Some("update") => cmd_update(&args[1..]),
        Some("app") => start_app(),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        None => start_app(),
        Some(other) => {
            if let Some((plugin_name, command)) = plugin::resolve_command(other) {
                let forwarded = args[1..].to_vec();
                let value = plugin::invoke(
                    &plugin_name,
                    "command",
                    json!({
                        "plugin": plugin_name,
                        "command": command,
                        "args": forwarded,
                        "cwd": std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()),
                    }),
                )?;
                print_plugin_result(&value);
                return Ok(());
            }
            bail!("unknown command '{other}'. Try: omnix --help")
        }
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
