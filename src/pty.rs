use anyhow::{bail, Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Travels with every terminal. The app writes this to `.omnix/agent-guide.md`
/// and exports the path as `OMNIX_HARNESS_GUIDE`.
const AGENT_GUIDE: &str = include_str!("../harness/AGENT.md");

pub fn install_guide(root: &Path) -> Result<()> {
    let path = root.join(".omnix").join("agent-guide.md");
    let changed = std::fs::read_to_string(&path)
        .map(|current| current != AGENT_GUIDE)
        .unwrap_or(true);
    if changed {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, AGENT_GUIDE)?;
    }
    Ok(())
}

fn browser_env(cmd: &mut CommandBuilder) {
    let guide = crate::root().join(".omnix").join("agent-guide.md");
    cmd.env("OMNIX_BROWSER", "lightpanda");
    cmd.env("OMNIX_HARNESS_GUIDE", guide.to_string_lossy().to_string());
    cmd.env("OMNIX_LIGHTPANDA", crate::browser::binary());
    if let Ok(exe) = std::env::current_exe() {
        cmd.env("OMNIX_BROWSER_MCP", exe.to_string_lossy().to_string());
    }
}

/// Each terminal is its own browser-session owner. An agent can then stop its
/// own Lightpanda MCP connection without touching another worker.
fn terminal_identity() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "term-{}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        nanos % 1_000_000
    )
}

fn escape_toml(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out
}

fn has_flag(argv: &[String], needle: &str) -> bool {
    argv.iter().any(|arg| arg == needle || arg.starts_with(&format!("{needle}=")))
}

/// Deliver the harness rules through each CLI's own supported mechanism. The
/// user's project files are never touched: Claude gets an extra system prompt,
/// Codex gets a developer-instructions config override, and Antigravity gets a
/// guide directory added to its workspace (it reads `GEMINI.md`/`AGENTS.md`).
pub fn harness_args(root: &Path, agent: &str, argv: &[String]) -> Result<Vec<String>> {
    install_guide(root)?;
    let guide_dir = root.join(".omnix").join("agent-guide");
    let mut out = Vec::new();
    match crate::agent::base_agent(agent) {
        "claude" => {
            if !has_flag(argv, "--append-system-prompt") {
                out.push("--append-system-prompt".into());
                out.push(AGENT_GUIDE.to_string());
            }
        }
        "codex" => {
            let already = argv
                .iter()
                .any(|arg| arg.contains("developer_instructions") || arg.contains("model_instructions_file"));
            if !already {
                out.push("-c".into());
                out.push(format!(
                    "developer_instructions=\"{}\"",
                    escape_toml(AGENT_GUIDE)
                ));
            }
        }
        "antigravity" => {
            std::fs::create_dir_all(&guide_dir)?;
            for name in ["GEMINI.md", "AGENTS.md"] {
                let path = guide_dir.join(name);
                let changed = std::fs::read_to_string(&path)
                    .map(|current| current != AGENT_GUIDE)
                    .unwrap_or(true);
                if changed {
                    std::fs::write(&path, AGENT_GUIDE)?;
                }
            }
            out.push("--add-dir".into());
            out.push(guide_dir.to_string_lossy().into_owned());
        }
        _ => {}
    }
    Ok(out)
}

pub struct Pty {
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub struct Spawned {
    pub pty: Pty,
    pub reader: Box<dyn Read + Send>,
    pub pid: u32,
}

fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    } else {
        "/bin/sh".into()
    }
}

/// Put the running binary's folder first on PATH. Agents run `omnix agent
/// send` from a menu-launched app, whose PATH rarely contains the install
/// folder (the macOS app bundle and a portable Windows build both need this).
fn path_with_exe_dir(exe: &Path, current: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    let dir = exe.parent()?;
    let mut dirs = vec![dir.to_path_buf()];
    if let Some(path) = current {
        dirs.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(dirs).ok()
}

fn open(cmd: CommandBuilder, cwd: &Path) -> Result<Spawned> {
    let mut cmd = cmd;
    cmd.cwd(cwd);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(path) = path_with_exe_dir(&exe, std::env::var_os("PATH")) {
            cmd.env("PATH", path);
        }
    }
    /* A desktop launch inherits no TERM, and a TUI that sees "dumb" or nothing
       draws monochrome. Every pty we open can do 256 colours and truecolor. */
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "omnix");
    let identity = terminal_identity();
    cmd.env("OMNIX_OWNER", &identity);
    cmd.env("OMNIX_BROWSER_SESSION", &identity);
    browser_env(&mut cmd);
    let pair = portable_pty::native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let child = pair.slave.spawn_command(cmd).context("spawn pty child")?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().context("pty reader")?;
    let writer = pair.master.take_writer().context("pty writer")?;
    let pid = child.process_id().unwrap_or(0);

    Ok(Spawned {
        pty: Pty {
            writer,
            master: pair.master,
            child,
        },
        reader,
        pid,
    })
}

/// Spawn a user-supplied CLI line, e.g. `codex` or `my-agent --fast`.
/// Whitespace splits only: no shell is involved, so metacharacters are inert.
pub fn spawn_command(line: &str, cwd: &Path) -> Result<Spawned> {
    let mut parts = line.split_whitespace();
    let bin = parts
        .next()
        .with_context(|| format!("empty command '{line}'"))?;
    let mut cmd = CommandBuilder::new(bin);
    for arg in parts {
        cmd.arg(arg);
    }
    open(cmd, cwd)
}

/// Resolve the binary and the full argv/env for an agent launch: saved extra
/// arguments first, then per-handoff flags, then resume flags, then the saved
/// environment.
pub fn agent_argv_at(
    home: &Path,
    root: &Path,
    name: &str,
    session: Option<&str>,
    extra: &[String],
) -> Result<(std::path::PathBuf, Vec<String>, Vec<(String, String)>)> {
    let bin = crate::agent::resolve(name)
        .or_else(|| crate::agent::launch_cmd(name).map(std::path::PathBuf::from))
        .with_context(|| format!("unknown agent '{name}'"))?;
    let cfg = crate::agent::config_for(root, name);
    if !cfg.enabled {
        bail!("agent '{name}' is disabled in Settings / Agents");
    }
    let mut argv = cfg.arg_list();
    let mut env = cfg.env_list();
    if let Some(profile) = crate::plugin::agent_profile(name) {
        argv.extend(profile.args);
        env.extend(profile.env);
    }
    if crate::agent::load_defaults(root).full_access {
        if let Some(flag) = crate::agent::full_access_flag(name) {
            argv.push(flag.to_string());
        }
    }
    argv.extend(extra.iter().cloned());
    let harness = harness_args(root, name, &argv)?;
    argv.extend(harness);
    if let Some(id) = session {
        argv.extend(crate::agent::resume_args(name, id)?);
    }
    env.extend(crate::agent::prepare_profile_at(home, root, name)?);
    Ok((bin, argv, env))
}

/// Claude asks "Is this a project you trust?" for every new folder and exits
/// when nothing answers. Omnix marks only its own `.omnix/worktrees/` checkouts
/// trusted, and records the previous value so `omnix` can undo it.
pub fn grant_claude_trust_at(home: &Path, worktree: &Path) -> Result<()> {
    if !worktree.components().any(|c| c.as_os_str() == ".omnix") {
        return Ok(());
    }
    let key = worktree.to_string_lossy().into_owned();
    let claude = home.join(".claude.json");
    let mut doc: serde_json::Value = match std::fs::read(&claude) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("parse {}", claude.display()))?,
        Err(_) => serde_json::json!({}),
    };
    if !doc.is_object() {
        bail!("{} is not a JSON object", claude.display());
    }
    let previous = doc["projects"][&key]["hasTrustDialogAccepted"].clone();
    let projects = doc
        .as_object_mut()
        .unwrap()
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let entry = projects
        .as_object_mut()
        .context("projects is not an object")?
        .entry(key.clone())
        .or_insert_with(|| serde_json::json!({}));
    entry["hasTrustDialogAccepted"] = serde_json::Value::Bool(true);
    let bytes = serde_json::to_vec_pretty(&doc)?;
    let tmp = claude.with_extension("json.omnix-tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &claude)?;

    let log_path = home.join(".omnix").join("trust-created.json");
    let mut log: serde_json::Value = std::fs::read(&log_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!({ "version": 1, "created": {} }));
    if !log["created"].get(&key).is_some() {
        log["created"][&key] = serde_json::json!({ "file": claude, "previous": previous });
        std::fs::create_dir_all(log_path.parent().unwrap())?;
        std::fs::write(&log_path, serde_json::to_vec_pretty(&log)?)?;
    }
    Ok(())
}

pub fn agent_argv_with(
    root: &Path,
    name: &str,
    session: Option<&str>,
    extra: &[String],
) -> Result<(std::path::PathBuf, Vec<String>, Vec<(String, String)>)> {
    agent_argv_at(&crate::control::home(), root, name, session, extra)
}

#[allow(dead_code)]
pub fn agent_argv(
    root: &Path,
    name: &str,
    session: Option<&str>,
) -> Result<(std::path::PathBuf, Vec<String>, Vec<(String, String)>)> {
    agent_argv_with(root, name, session, &[])
}

pub fn spawn_agent_with(
    root: &Path,
    name: &str,
    session: Option<&str>,
    cwd: &Path,
    extra: &[String],
    extra_env: &[(String, String)],
) -> Result<Spawned> {
    let (bin, argv, env) = agent_argv_with(root, name, session, extra)?;
    if crate::agent::base_agent(name) == "claude" {
        grant_claude_trust_at(&crate::control::home(), cwd)?;
    }
    let mut cmd = CommandBuilder::new(bin);
    for arg in argv {
        cmd.arg(arg);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    open(cmd, cwd)
}

/// Open the agent's own sign-in flow inside a profile. Omnix passes only the
/// config folder env; it never reads or stores the token.
pub fn spawn_login(
    root: &Path,
    name: &str,
    cwd: &Path,
) -> Result<(Spawned, Option<String>)> {
    let (args, type_login) = crate::agent::login_parts(name)?;
    let bin = crate::agent::resolve(name)
        .or_else(|| crate::agent::launch_cmd(name).map(std::path::PathBuf::from))
        .with_context(|| format!("unknown agent '{name}'"))?;
    let mut cmd = CommandBuilder::new(bin);
    for arg in args {
        cmd.arg(arg);
    }
    for (key, value) in crate::agent::config_for(root, name).env_list() {
        cmd.env(key, value);
    }
    for (key, value) in crate::agent::prepare_profile(root, name)? {
        cmd.env(key, value);
    }
    let spawned = open(cmd, cwd)?;
    let prompt = type_login.then(|| "/login".to_string());
    Ok((spawned, prompt))
}

/// Spawn a shell, or resume an agent TUI directly in the pty.
/// Agent args come from `agent::resume_args`, so the id is validated first.
pub fn spawn(shell: Option<&str>, cwd: &Path, agent: Option<(&str, &str)>) -> Result<Spawned> {
    let cmd = match agent {
        Some((name, session)) => {
            let bin = crate::agent::resolve(name)
                .or_else(|| crate::agent::launch_cmd(name).map(std::path::PathBuf::from))
                .with_context(|| format!("unknown agent '{name}'"))?;
            let mut cmd = CommandBuilder::new(bin);
            for arg in crate::agent::resume_args(name, session)? {
                cmd.arg(arg);
            }
            for (key, value) in crate::agent::config_for(&crate::root(), name).env_list() {
                cmd.env(key, value);
            }
            for (key, value) in crate::agent::prepare_profile(&crate::root(), name)? {
                cmd.env(key, value);
            }
            cmd
        }
        None => {
            let shell = shell
                .map(str::to_string)
                .or_else(|| std::env::var("SHELL").ok())
                .unwrap_or_else(default_shell);
            CommandBuilder::new(shell)
        }
    };
    open(cmd, cwd)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::fs;

    #[test]
    fn agent_argv_applies_saved_settings_before_resume_flags() {
        let root = std::env::temp_dir().join(format!("omnix-argv-{}", std::process::id()));
        let dir = root.join(".omnix");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agents.json"),
            r#"{"codex":{"args":"--fast","env":"FOO=1"}}"#,
        )
        .unwrap();

        let (_, argv, env) = agent_argv(&root, "codex", Some("abc-123")).unwrap();
        assert_eq!(argv.first().map(String::as_str), Some("--fast"));
        let harness = argv
            .iter()
            .position(|arg| arg == "-c")
            .expect("codex gets developer_instructions");
        assert!(
            argv[harness + 1].starts_with("developer_instructions=\""),
            "{argv:?}"
        );
        assert_eq!(&argv[argv.len() - 2..], ["resume", "abc-123"]);
        assert_eq!(env, vec![("FOO".to_string(), "1".to_string())]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn disabled_agent_is_not_launched() {
        let root = std::env::temp_dir().join(format!("omnix-argv-off-{}", std::process::id()));
        let dir = root.join(".omnix");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agents.json"),
            r#"{"codex":{"enabled":false}}"#,
        )
        .unwrap();
        assert!(agent_argv(&root, "codex", None).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn full_access_default_appends_known_flag_only() {
        let root = std::env::temp_dir().join(format!("omnix-argv-full-{}", std::process::id()));
        let dir = root.join(".omnix");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agents.json"), r#"{"codex":{"args":"--fast"}}"#).unwrap();
        std::fs::write(dir.join("agent-defaults.json"), r#"{"full_access":true}"#).unwrap();
        let (_, argv, _) = agent_argv(&root, "codex", Some("abc")).unwrap();
        assert_eq!(
            &argv[..2],
            ["--fast", "--dangerously-bypass-approvals-and-sandbox"]
        );
        assert!(argv.iter().any(|arg| arg == "-c"), "{argv:?}");
        assert_eq!(&argv[argv.len() - 2..], ["resume", "abc"]);
        // an agent without a known flag is left untouched
        std::fs::write(dir.join("agents.json"), r#"{}"#).unwrap();
        let (_, argv, _) = agent_argv(&root, "kiro", None).unwrap();
        assert!(argv.is_empty(), "unknown flag must not be guessed: {argv:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn profile_env_reaches_the_agent_argv() {
        let base = std::env::temp_dir().join(format!("omnix-argv-profile-{}", std::process::id()));
        let root = base.join("repo");
        let home = base.join("home");
        std::fs::create_dir_all(root.join(".omnix")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            root.join(".omnix").join("agents.json"),
            r#"{"codex:akun2":{"env":"FOO=1"}}"#,
        )
        .unwrap();
        let (_, argv, env) = agent_argv_at(&home, &root, "codex:akun2", None, &[]).unwrap();
        // No user args, but codex still receives the harness guide.
        assert_eq!(argv.first().map(String::as_str), Some("-c"), "{argv:?}");
        let dir = home.join(".omnix").join("profiles").join("codex").join("akun2");
        assert!(env.contains(&("FOO".to_string(), "1".to_string())));
        assert!(
            env.contains(&("CODEX_HOME".to_string(), dir.to_string_lossy().into_owned())),
            "env was {env:?}"
        );
        assert!(dir.is_dir(), "profile folder must exist");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn harness_guide_uses_each_cli_supported_flag() {
        let root = std::env::temp_dir().join(format!("omnix-harness-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        let claude = harness_args(&root, "claude", &[]).unwrap();
        assert_eq!(claude[0], "--append-system-prompt");
        assert!(claude[1].contains("Browser: Lightpanda only"));
        let claude_again = harness_args(
            &root,
            "claude",
            &["--append-system-prompt".into(), "custom".into()],
        )
        .unwrap();
        assert!(claude_again.is_empty(), "a user flag wins: {claude_again:?}");

        let codex = harness_args(&root, "codex", &[]).unwrap();
        assert_eq!(codex[0], "-c");
        assert!(codex[1].starts_with("developer_instructions=\""));
        assert!(codex[1].ends_with('"'));

        let antigravity = harness_args(&root, "antigravity", &[]).unwrap();
        assert_eq!(antigravity[0], "--add-dir");
        let dir = std::path::PathBuf::from(&antigravity[1]);
        assert!(dir.join("GEMINI.md").is_file());
        assert!(dir.join("AGENTS.md").is_file());
        assert!(root.join(".omnix").join("agent-guide.md").is_file());

        let other = harness_args(&root, "kiro", &[]).unwrap();
        assert!(other.is_empty(), "unknown CLIs keep their argv: {other:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn exe_dir_goes_first_on_path() {
        let exe = Path::new("/opt/omnix/bin/omnix");
        let current = std::env::join_paths(["/usr/bin", "/bin"]).unwrap();
        let joined = path_with_exe_dir(exe, Some(current)).unwrap();
        let dirs: Vec<_> = std::env::split_paths(&joined).collect();
        assert_eq!(
            dirs.first().map(|p| p.to_path_buf()),
            Some(std::path::PathBuf::from("/opt/omnix/bin"))
        );
        assert!(dirs.contains(&std::path::PathBuf::from("/usr/bin")));
    }

    #[test]
    fn spawn_shell_and_read_output() {        let dir = std::env::temp_dir();
        let mut spawned = spawn(None, &dir, None).expect("spawn shell");
        let mut reader = spawned.reader;
        {
            use std::io::Write;
            spawned
                .pty
                .writer
                .write_all(b"printf 'omnix-pty-ok'\n")
                .unwrap();
            spawned.pty.writer.flush().unwrap();
        }
        let mut buf = [0u8; 4096];
        let mut seen = String::new();
        for _ in 0..200 {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    seen.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if seen.contains("omnix-pty-ok") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = spawned.pty.child.kill();
        assert!(seen.contains("omnix-pty-ok"), "pty output was {seen:?}");
    }

    #[test]
    fn claude_trust_is_granted_only_to_omnix_worktrees_and_remembers_the_old_value() {
        let home = std::env::temp_dir().join(format!("omnix-trust-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let claude = home.join(".claude.json");
        fs::write(
            &claude,
            r#"{"numStartups":3,"projects":{"/repo/.omnix/worktrees/fix-a":{"hasTrustDialogAccepted":false,"allowedTools":[]}}}"#,
        )
        .unwrap();

        let own = Path::new("/repo/.omnix/worktrees/fix-b");
        let other = Path::new("/home/me/project");
        grant_claude_trust_at(&home, own).unwrap();
        grant_claude_trust_at(&home, other).unwrap();

        let doc: Value = serde_json::from_slice(&fs::read(&claude).unwrap()).unwrap();
        assert_eq!(doc["numStartups"], 3, "unrelated keys stay");
        assert_eq!(doc["projects"]["/repo/.omnix/worktrees/fix-b"]["hasTrustDialogAccepted"], true);
        assert!(doc["projects"].get("/home/me/project").is_none(), "non-omnix path is untouched");
        assert_eq!(doc["projects"]["/repo/.omnix/worktrees/fix-a"]["allowedTools"], json!([]));

        let log: Value = serde_json::from_slice(&fs::read(home.join(".omnix").join("trust-created.json")).unwrap()).unwrap();
        let entry = &log["created"]["/repo/.omnix/worktrees/fix-b"];
        assert_eq!(entry["previous"], Value::Null, "undo knows there was no prior value");
        let _ = fs::remove_dir_all(&home);
    }
}
