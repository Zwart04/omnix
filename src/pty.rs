use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::Path;

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

fn open(cmd: CommandBuilder, cwd: &Path) -> Result<Spawned> {
    let mut cmd = cmd;
    cmd.cwd(cwd);
    /* A desktop launch inherits no TERM, and a TUI that sees "dumb" or nothing
       draws monochrome. Every pty we open can do 256 colours and truecolor. */
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "omnix");
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
/// arguments first, then resume flags, then the saved environment.
pub fn agent_argv(
    root: &Path,
    name: &str,
    session: Option<&str>,
) -> Result<(std::path::PathBuf, Vec<String>, Vec<(String, String)>)> {
    let bin = crate::agent::resolve(name)
        .or_else(|| crate::agent::launch_cmd(name).map(std::path::PathBuf::from))
        .with_context(|| format!("unknown agent '{name}'"))?;
    let cfg = crate::agent::load_settings(root)
        .remove(name)
        .unwrap_or_default();
    let mut argv = cfg.arg_list();
    if let Some(id) = session {
        argv.extend(crate::agent::resume_args(name, id)?);
    }
    Ok((bin, argv, cfg.env_list()))
}

/// Spawn an agent TUI with its saved launch overrides, optionally resuming.
/// Arguments come from the config, never from a shell, so they stay inert.
pub fn spawn_agent(root: &Path, name: &str, session: Option<&str>, cwd: &Path) -> Result<Spawned> {
    let (bin, argv, env) = agent_argv(root, name, session)?;
    let mut cmd = CommandBuilder::new(bin);
    for arg in argv {
        cmd.arg(arg);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    open(cmd, cwd)
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
        assert_eq!(argv, vec!["--fast", "resume", "abc-123"]);
        assert_eq!(env, vec![("FOO".to_string(), "1".to_string())]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn spawn_shell_and_read_output() {
        let dir = std::env::temp_dir();
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
}
