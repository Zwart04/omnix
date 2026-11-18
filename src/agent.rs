use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Menu order mirrors Orca: the agents people actually run first.
pub const AGENTS: &[&str] = &[
    "opencode2",
    "codex",
    "opencode",
    "antigravity",
    "command-code",
    "claude",
    "cursor",
    "gemini",
    "grok",
    "copilot",
    "qwen-code",
    "pi",
    "kimi",
    "kiro",
    "rovo",
];

/// Per-agent launch overrides, persisted as `.omnix/agents.json` under the
/// app root. `args` is appended before any resume flags; `env` is
/// whitespace-separated `KEY=VALUE` pairs. Values containing spaces are not
/// supported (ponytail: quote-aware split the day someone needs it).
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub env: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            args: String::new(),
            env: String::new(),
            enabled: true,
        }
    }
}

/// App-wide launch defaults, persisted next to the per-agent settings. The
/// permissions switch mirrors Orca: a new agent either asks or starts with the
/// agent's own full-access flag.
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentDefaults {
    #[serde(default)]
    pub full_access: bool,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self { full_access: false }
    }
}

impl AgentConfig {
    pub fn arg_list(&self) -> Vec<String> {
        self.args.split_whitespace().map(str::to_string).collect()
    }

    pub fn env_list(&self) -> Vec<(String, String)> {
        self.env
            .split_whitespace()
            .filter_map(|pair| pair.split_once('='))
            .filter(|(key, _)| valid_env_key(key))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }
}

fn valid_env_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with(|c: char| c.is_ascii_digit())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn settings_path(root: &Path) -> PathBuf {
    root.join(".omnix").join("agents.json")
}

pub fn defaults_path(root: &Path) -> PathBuf {
    root.join(".omnix").join("agent-defaults.json")
}

pub fn load_defaults(root: &Path) -> AgentDefaults {
    std::fs::read(defaults_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_defaults(root: &Path, defaults: &AgentDefaults) -> Result<()> {
    let path = defaults_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(defaults)?)?;
    Ok(())
}

/// Full-access launch flag each CLI understands. An unknown CLI gets no flag
/// rather than a guessed one, so enabling the default never breaks a launch.
pub fn full_access_flag(agent: &str) -> Option<&'static str> {
    Some(match agent {
        "claude" => "--dangerously-skip-permissions",
        "codex" => "--dangerously-bypass-approvals-and-sandbox",
        "gemini" | "qwen-code" => "--yolo",
        "copilot" => "--allow-all-tools",
        _ => return None,
    })
}

pub fn load_settings(root: &Path) -> BTreeMap<String, AgentConfig> {
    std::fs::read(settings_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(root: &Path, map: &BTreeMap<String, AgentConfig>) -> Result<()> {
    let path = settings_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(map)?)?;
    Ok(())
}

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
        "antigravity" => "agy",
        "rovo" => "rovo",
        "command-code" => "command-code",
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

/// Install locations a desktop-launched app rarely inherits in PATH. A menu
/// launch gets the session PATH, which on most setups misses per-user bin dirs,
/// so an agent installed there would look missing.
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        let home = PathBuf::from(home);
        for rel in [
            ".opencode/bin",
            ".local/bin",
            ".bun/bin",
            ".cargo/bin",
            ".deno/bin",
            ".npm-global/bin",
            "go/bin",
        ] {
            dirs.push(home.join(rel));
        }
        // nvm keeps one bin dir per node version.
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm").join("versions").join("node")) {
            for entry in entries.flatten() {
                dirs.push(entry.path().join("bin"));
            }
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    dirs
}

fn candidate_names(bin: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
        std::iter::once(bin.to_string())
            .chain(
                exts.split(';')
                    .filter(|e| !e.is_empty())
                    .map(|e| format!("{bin}{e}")),
            )
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![bin.to_string()]
    }
}

/// Resolve an agent binary across PATH plus the per-user install dirs the GUI
/// session usually lacks. Returns the absolute path used for spawning.
pub fn resolve(agent: &str) -> Option<PathBuf> {
    let bin = launch_cmd(agent)?;
    let names = candidate_names(bin);
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(extra_dirs());
    dirs.into_iter()
        .find_map(|dir| names.iter().map(|name| dir.join(name)).find(|p| executable(p)))
}

/// True when the agent CLI resolves on PATH. The menu lists only these.
pub fn available(agent: &str) -> bool {
    resolve(agent).is_some()
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
    fn extra_dirs_cover_per_user_installs() {
        let dirs = extra_dirs();
        assert!(!dirs.is_empty(), "home-based install dirs are searched");
        assert!(
            dirs.iter().any(|d| d.ends_with(".opencode/bin")),
            "opencode ships in ~/.opencode/bin"
        );
    }

    #[test]
    fn config_parses_args_and_env() {
        let cfg = AgentConfig {
            args: "--dangerously-skip-permissions --fast".into(),
            env: "FOO=1 NOPE BAR=two=3 9X=y".into(),
            enabled: true,
        };
        assert_eq!(cfg.arg_list().len(), 2);
        assert_eq!(
            cfg.env_list(),
            vec![
                ("FOO".to_string(), "1".to_string()),
                ("BAR".to_string(), "two=3".to_string())
            ]
        );
    }

    #[test]
    fn rejects_injection_shaped_ids() {
        assert!(resume_args("codex", "-rf /").is_err());
        assert!(resume_args("codex", "a\nb").is_err());
        assert!(resume_args("codex", "").is_err());
    }
}
