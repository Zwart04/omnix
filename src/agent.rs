use anyhow::{Result, bail};
use std::process::{Child, Command, Stdio};

pub const AGENTS: &[&str] = &[
    "claude",
    "codex",
    "opencode",
    "opencode2",
    "cursor",
    "gemini",
    "grok",
    "copilot",
    "qwen-code",
    "pi",
    "kimi",
    "kiro",
    "antigravity",
    "rovo",
];

pub fn launch_cmd(agent: &str) -> Option<&'static str> {
    Some(match agent {
        "claude" => "claude",
        "codex" => "codex",
        "opencode" => "opencode",
        "opencode2" => "opencode",
        "cursor" => "cursor-agent",
        "gemini" => "gemini",
        "grok" => "grok",
        "copilot" => "copilot",
        "qwen-code" => "qwen",
        "pi" => "pi",
        "kimi" => "kimi",
        "kiro" => "kiro",
        "antigravity" => "antigravity",
        "rovo" => "rovo",
        _ => return None,
    })
}

fn executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// True when the agent CLI resolves on PATH. The UI greys out the rest instead
/// of letting the spawn fail with a raw pty error.
pub fn available(agent: &str) -> bool {
    let Some(bin) = launch_cmd(agent) else {
        return false;
    };
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    #[cfg(windows)]
    let names: Vec<String> = {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
        std::iter::once(bin.to_string())
            .chain(
                exts.split(';')
                    .filter(|e| !e.is_empty())
                    .map(|e| format!("{bin}{e}")),
            )
            .collect()
    };
    #[cfg(not(windows))]
    let names = [bin.to_string()];

    std::env::split_paths(&path)
        .any(|dir| names.iter().any(|name| executable(&dir.join(name))))
}

fn valid_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 512
        && !session_id.starts_with('-')
        && !session_id.chars().any(|c| c.is_control())
}

/// Mirrors Orca's per-agent resume flag table (see docs/orca-evidence.md).
pub fn resume_args(agent: &str, session_id: &str) -> Result<Vec<String>> {
    if launch_cmd(agent).is_none() {
        bail!("unknown agent '{agent}' (known: {})", AGENTS.join(", "));
    }
    if !valid_id(session_id) {
        bail!("invalid session id");
    }
    let id = session_id.to_string();
    Ok(match agent {
        "codex" => vec!["resume".into(), id],
        "rovo" => vec!["--restore".into(), id],
        "opencode" | "opencode2" | "pi" | "kimi" => vec!["--session".into(), id],
        "copilot" => vec![format!("--resume={id}")],
        "cline" => vec!["--id".into(), id],
        "antigravity" => vec!["--conversation".into(), id],
        "kiro" => vec!["--resume-id".into(), id],
        _ => vec!["--resume".into(), id],
    })
}

pub fn resume_command(agent: &str, session_id: &str) -> Result<String> {
    let base = launch_cmd(agent).expect("validated above");
    let args = resume_args(agent, session_id)?;
    Ok(format!("{base} {}", args.join(" ")))
}

/// Spawn without touching a shell, so session ids cannot inject commands.
pub fn spawn(agent: &str, session_id: Option<&str>, cwd: &str) -> Result<Child> {
    let base = launch_cmd(agent).ok_or_else(|| anyhow::anyhow!("unknown agent '{agent}'"))?;
    let mut cmd = Command::new(base);
    if let Some(id) = session_id {
        cmd.args(resume_args(agent, id)?);
    }
    Ok(cmd
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_flags_match_orca_table() {
        assert_eq!(
            resume_command("codex", "abc-123").unwrap(),
            "codex resume abc-123"
        );
        assert_eq!(
            resume_command("opencode", "ses_1").unwrap(),
            "opencode --session ses_1"
        );
        assert_eq!(
            resume_command("claude", "u-1").unwrap(),
            "claude --resume u-1"
        );
        assert!(resume_args("copilot", "x").unwrap()[0].starts_with("--resume="));
    }

    #[test]
    fn rejects_injection_shaped_ids() {
        assert!(resume_args("codex", "-rf /").is_err());
        assert!(resume_args("codex", "a\nb").is_err());
        assert!(resume_args("codex", "").is_err());
    }
}
