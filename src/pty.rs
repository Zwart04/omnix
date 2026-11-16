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

/// Spawn a shell, or resume an agent TUI directly in the pty.
/// Agent args come from `agent::resume_args`, so the id is validated first.
pub fn spawn(shell: Option<&str>, cwd: &Path, agent: Option<(&str, &str)>) -> Result<Spawned> {
    let pair = portable_pty::native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = match agent {
        Some((name, session)) => {
            let bin = crate::agent::launch_cmd(name)
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
    cmd.cwd(cwd);

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

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
