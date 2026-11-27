use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Menu order mirrors Orca: the agents people actually run first. The tail
/// holds the newer viral CLIs; the Project Team picker keeps this list behind
/// a "More" button so every supported agent stays reachable without turning
/// the dialog into a wall of pills.
pub const AGENTS: &[&str] = &[
    "opencode2",
    "codex",
    "opencode",
    "omp",
    "claude",
    "antigravity",
    "command-code",
    "cursor",
    "gemini",
    "grok",
    "copilot",
    "qwen-code",
    "pi",
    "kimi",
    "kiro",
    "rovo",
    "amp",
    "droid",
    "crush",
    "aider",
    "goose",
    "openhands",
    "codebuff",
    "plandex",
    "junie",
    "amazon-q",
    "continue",
    "forge",
    "vibe",
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
    /// Shown in menus and tabs; empty means the profile name.
    #[serde(default)]
    pub label: String,
    /// Login/config folder override. Empty means `~/.omnix/profiles/<agent>/<profile>`.
    #[serde(default)]
    pub config_dir: String,
    /// Claude only: share the base account's `projects/` memory with a symlink.
    #[serde(default)]
    pub share_projects: bool,
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
            label: String::new(),
            config_dir: String::new(),
            share_projects: false,
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
    Some(match base_agent(agent) {
        "claude" => "--dangerously-skip-permissions",
        "codex" => "--dangerously-bypass-approvals-and-sandbox",
        "antigravity" => "--dangerously-skip-permissions",
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

/// `codex:akun2` names one profile of the `codex` CLI. The base name must be a
/// known agent; the profile is a short lowercase id used for the config folder.
pub fn split_agent(agent: &str) -> Option<(&str, Option<&str>)> {
    let (base, profile) = match agent.split_once(':') {
        Some((base, profile)) => (base, Some(profile)),
        None => (agent, None),
    };
    if !AGENTS.contains(&base) {
        let plugin_agent = profile.is_none() && crate::plugin::is_agent(base);
        if !plugin_agent {
            return None;
        }
    }
    if let Some(profile) = profile {
        if !valid_profile(profile) {
            return None;
        }
    }
    Some((base, profile))
}

pub fn valid_profile(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn base_agent(agent: &str) -> &str {
    split_agent(agent).map(|(base, _)| base).unwrap_or(agent)
}

/// True for a built-in agent or a plugin agent, including `name:profile`.
pub fn known(agent: &str) -> bool {
    split_agent(agent).is_some()
}

pub fn profile_of(agent: &str) -> Option<&str> {
    split_agent(agent).and_then(|(_, profile)| profile)
}

/// The official env var each CLI uses to move its login/config folder. An
/// agent missing here has no verified isolation knob (Antigravity today), so a
/// profile only changes args/env and stays on the shared login.
pub fn profile_env_var(agent: &str) -> Option<&'static str> {
    match base_agent(agent) {
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        "codex" => Some("CODEX_HOME"),
        _ => None,
    }
}

fn expand_dir(value: &str, home: &Path) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let path = if value == "~" {
        home.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(value)
    };
    if path.is_absolute() {
        Some(path)
    } else {
        Some(home.join(path))
    }
}

/// Login/config folder for one profile. The default lives under the user's
/// Omnix home, never inside the repository, so two repos share one login.
pub fn profile_dir_at(home: &Path, root: &Path, agent: &str) -> Option<PathBuf> {
    let (base, profile) = split_agent(agent)?;
    let profile = profile?;
    if profile_env_var(base).is_none() {
        return None;
    }
    let cfg = load_settings(root).remove(agent).unwrap_or_default();
    Some(expand_dir(&cfg.config_dir, home).unwrap_or_else(|| {
        home.join(".omnix")
            .join("profiles")
            .join(base)
            .join(profile)
    }))
}

fn link_shared(target: &Path, link: &Path) -> Result<()> {
    if !target.exists() {
        return Ok(());
    }
    if link.symlink_metadata().is_ok() {
        return Ok(());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(not(unix))]
    {
        let _ = (target, link);
        bail!("profile symlinks are only supported on Unix");
    }
    Ok(())
}

/// Create the profile folder and the Claude rule symlinks, then return the env
/// that points the CLI at it. Tokens are never read or copied.
pub fn prepare_profile_at(
    home: &Path,
    root: &Path,
    agent: &str,
) -> Result<Vec<(String, String)>> {
    let Some((base, Some(_profile))) = split_agent(agent) else {
        return Ok(Vec::new());
    };
    let Some(var) = profile_env_var(base) else {
        return Ok(Vec::new());
    };
    let dir = profile_dir_at(home, root, agent).expect("profile var implies a dir");
    std::fs::create_dir_all(&dir)?;
    if base == "claude" {
        let shared = home.join(".claude");
        link_shared(&shared.join("skills"), &dir.join("skills"))?;
        link_shared(&shared.join("CLAUDE.md"), &dir.join("CLAUDE.md"))?;
        let cfg = load_settings(root).remove(agent).unwrap_or_default();
        let projects = dir.join("projects");
        if cfg.share_projects {
            link_shared(&shared.join("projects"), &projects)?;
        } else {
            std::fs::create_dir_all(&projects)?;
        }
    }
    Ok(vec![(var.to_string(), dir.to_string_lossy().into_owned())])
}

pub fn prepare_profile(root: &Path, agent: &str) -> Result<Vec<(String, String)>> {
    prepare_profile_at(&crate::control::home(), root, agent)
}

/// JSON rows for the menu and the Settings list. Only real profile keys are
/// returned; base agents keep their existing row shape.
pub fn profile_rows(root: &Path) -> Vec<serde_json::Value> {
    let home = crate::control::home();
    let merged = load_settings(root);
    merged
        .iter()
        .filter_map(|(name, cfg)| {
            let (base, profile) = split_agent(name)?;
            let profile = profile?;
            let dir = profile_dir_at(&home, root, name)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            let label = if cfg.label.trim().is_empty() {
                profile.to_string()
            } else {
                cfg.label.trim().chars().take(80).collect()
            };
            Some(serde_json::json!({
                "name": name,
                "base": base,
                "profile": profile,
                "label": label,
                "bin": launch_cmd(name),
                "available": available(base),
                "enabled": config_for(root, name).enabled,
                "args": cfg.args,
                "env": cfg.env,
                "configDir": dir,
                "shareProjects": cfg.share_projects,
                "supportsConfig": profile_env_var(base).is_some(),
            }))
        })
        .collect()
}

/// Saved profile settings merged over the base agent's settings. A profile
/// with empty args/env inherits both, so the owner only writes the difference.
pub fn config_for(root: &Path, agent: &str) -> AgentConfig {
    let saved = load_settings(root);
    let (base, profile) = split_agent(agent).unwrap_or((agent, None));
    let mut cfg = saved.get(base).cloned().unwrap_or_default();
    let Some(_profile) = profile else {
        return cfg;
    };
    let Some(own) = saved.get(agent) else {
        return cfg;
    };
    cfg.enabled = cfg.enabled && own.enabled;
    cfg.label = own.label.clone();
    cfg.config_dir = own.config_dir.clone();
    cfg.share_projects = own.share_projects;
    if !own.args.trim().is_empty() {
        cfg.args = if cfg.args.trim().is_empty() {
            own.args.clone()
        } else {
            format!("{} {}", cfg.args.trim_end(), own.args.trim())
        };
    }
    if !own.env.trim().is_empty() {
        cfg.env = if cfg.env.trim().is_empty() {
            own.env.clone()
        } else {
            format!("{} {}", cfg.env.trim_end(), own.env.trim())
        };
    }
    cfg
}

/// Per-agent login argv. Claude opens its TUI and gets `/login` typed for it;
/// Codex has a real subcommand; Antigravity shows its own sign-in screen.
pub fn login_parts(agent: &str) -> Result<(Vec<String>, bool)> {
    let Some((base, _)) = split_agent(agent) else {
        bail!("unknown agent '{agent}' (known: {})", AGENTS.join(", "));
    };
    Ok(match base {
        "codex" => (vec!["login".to_string()], false),
        "claude" => (Vec::new(), true),
        "antigravity" => (Vec::new(), false),
        _ => bail!("sign-in is only wired for codex, claude, and antigravity"),
    })
}

pub fn launch_cmd(agent: &str) -> Option<&'static str> {
    let (agent, _) = split_agent(agent)?;
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
        "omp" => "omp",
        "amp" => "amp",
        "droid" => "droid",
        "crush" => "crush",
        "aider" => "aider",
        "goose" => "goose",
        "openhands" => "openhands",
        "codebuff" => "codebuff",
        "plandex" => "plandex",
        "junie" => "junie",
        "amazon-q" => "q",
        "continue" => "cn",
        "forge" => "forge",
        "vibe" => "vibe",
        _ => return plugin_launch_cmd(agent),
    })
}

/// Plugin agent binaries live in a manifest, not a static table. Leak one copy
/// per name and cache it: `launch_cmd` is called on every pane open, but the
/// process itself still starts only when the owner launches that agent.
fn plugin_launch_cmd(name: &str) -> Option<&'static str> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().ok()?;
    if let Some(bin) = map.get(name) {
        return Some(*bin);
    }
    let bin: &'static str = Box::leak(crate::plugin::agent_bin(name)?.into_boxed_str());
    map.insert(name.to_string(), bin);
    Some(bin)
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
    if let Some(profile) = crate::plugin::agent_profile(agent) {
        let path = PathBuf::from(&profile.bin);
        if path.is_absolute() {
            return executable(&path).then_some(path);
        }
        if profile.bin.contains('/') || profile.bin.contains('\\') {
            let path = profile.dir.join(&profile.bin);
            return executable(&path).then_some(path);
        }
    }
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
    if let Some(args) = crate::plugin::resume_args(agent, &id) {
        return Ok(args);
    }
    Ok(match base_agent(agent) {
        "codex" => vec!["resume".into(), id],
        "rovo" => vec!["--restore".into(), id],
        "opencode" | "opencode2" | "pi" | "kimi" | "omp" | "crush" => {
            vec!["--session".into(), id]
        }
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

/// A UUID v4 generated in-process so Omnix never shells out to `uuidgen`,
/// which is missing on stock laptops.
pub fn new_session_id() -> String {
    let mut bytes = [0u8; 16];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| {
            use std::io::Read;
            file.read_exact(&mut bytes)
        })
        .is_ok();
    if !filled {
        let mut state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            ^ ((std::process::id() as u64) << 32)
            ^ 0x9E37_79B9_7F4A_7C15;
        for chunk in bytes.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let le = state.to_le_bytes();
            for (dst, src) in chunk.iter_mut().zip(le.iter()) {
                *dst = *src;
            }
        }
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = |slice: &[u8]| slice.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[0..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..16])
    )
}

/// Claude accepts `--session-id <uuid>` to name a fresh conversation, so the
/// pane can record its id before the first reply arrives.
pub fn new_session_args(agent: &str, session_id: &str) -> Vec<String> {
    match base_agent(agent) {
        "claude" if valid_id(session_id) => {
            vec!["--session-id".to_string(), session_id.to_string()]
        }
        _ => Vec::new(),
    }
}

/// Pull a session id a CLI prints on its own line (`session id: <token>`).
pub fn extract_session_id(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for needle in ["session id:", "session_id:", "conversation_id:"] {
        if let Some(pos) = lower.find(needle) {
            let rest = line[pos + needle.len()..].trim();
            let token: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                .collect();
            if token.len() >= 6 {
                return Some(token);
            }
        }
    }
    None
}

/// Per-handoff model knobs. Values are validated as single argv tokens before
/// they ever reach a spawn, so a model name cannot become a second flag.
#[derive(Debug, Default, Clone)]
pub struct LaunchOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: Option<String>,
    /// First turn for CLIs that can take it as an argv value. This is more
    /// reliable than typing a long brief into a TUI that is still booting.
    pub prompt: Option<String>,
}

fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '@' | '+' | '-'))
}

/// Validate and build the extra argv for the three CLIs the orchestrator
/// drives. Unknown agents simply get no extra flags.
pub fn extra_args(agent: &str, options: &LaunchOptions) -> Result<Vec<String>> {
    if launch_cmd(agent).is_none() {
        bail!("unknown agent '{agent}' (known: {})", AGENTS.join(", "));
    }
    if let Some(model) = &options.model {
        if !valid_model(model) {
            bail!("--model must be 1-80 chars of letters, digits, . _ : / @ +");
        }
    }
    if let Some(effort) = &options.effort {
        if !matches!(
            effort.as_str(),
            "minimal" | "low" | "medium" | "high" | "xhigh" | "none"
        ) {
            bail!("--effort must be minimal, low, medium, high, xhigh, or none");
        }
    }
    if let Some(mode) = &options.mode {
        if !matches!(mode.as_str(), "plan" | "edit" | "full") {
            bail!("--mode must be plan, edit, or full");
        }
    }
    let base = base_agent(agent);
    if options.model.is_some()
        && !matches!(base, "codex" | "claude" | "antigravity" | "opencode" | "opencode2")
    {
        bail!("--model is only supported for codex, claude, antigravity, and opencode");
    }
    if options.effort.is_some() && !matches!(base, "codex" | "antigravity" | "claude") {
        bail!("--effort is only supported for codex, antigravity, and claude");
    }
    if options.mode.is_some() && !matches!(base, "antigravity" | "claude") {
        bail!("--mode is only supported for antigravity and claude");
    }
    if let Some(prompt) = &options.prompt {
        if prompt.trim().is_empty() {
            bail!("prompt must not be empty");
        }
        if prompt.chars().count() > 20000 {
            bail!("prompt must be at most 20000 characters");
        }
        if !matches!(base_agent(agent), "opencode" | "opencode2") {
            bail!("--prompt is only supported for opencode");
        }
    }
    let mut out = Vec::new();
    match base_agent(agent) {
        "codex" => {
            if let Some(model) = &options.model {
                out.push("-m".into());
                out.push(model.clone());
            }
            if let Some(effort) = &options.effort {
                out.push("-c".into());
                out.push(format!("model_reasoning_effort={effort}"));
            }
        }
        "antigravity" => {
            if let Some(model) = &options.model {
                out.push("--model".into());
                out.push(model.clone());
            }
            if let Some(effort) = &options.effort {
                out.push("--effort".into());
                out.push(effort.clone());
            }
            if let Some(mode) = &options.mode {
                let mapped = match mode.as_str() {
                    "edit" => "accept-edits",
                    "full" => "auto",
                    other => other,
                };
                out.push("--mode".into());
                out.push(mapped.into());
            }
        }
        "claude" => {
            if let Some(model) = &options.model {
                out.push("--model".into());
                out.push(model.clone());
            }
            if let Some(effort) = &options.effort {
                out.push("--effort".into());
                out.push(effort.clone());
            }
            if let Some(mode) = &options.mode {
                let mapped = match mode.as_str() {
                    "edit" => "acceptEdits",
                    "full" => "bypassPermissions",
                    other => other,
                };
                out.push("--permission-mode".into());
                out.push(mapped.into());
            }
        }
        "opencode" | "opencode2" => {
            if let Some(prompt) = &options.prompt {
                out.push("--prompt".into());
                out.push(prompt.clone());
            }
        }
        _ => {}
    }
    Ok(out)
}

/// argv for a non-interactive run. Only the four CLIs with a verified
/// headless mode are accepted; the rest fall back to opening a pane. The
/// caller redirects stdout/stderr to the handoff log.
pub fn headless_args(agent: &str, options: &LaunchOptions, brief: &str) -> Result<Vec<String>> {
    if brief.trim().is_empty() {
        bail!("headless brief must not be empty");
    }
    if brief.chars().count() > 200_000 {
        bail!("headless brief must be at most 200000 characters");
    }
    let extra = extra_args(agent, options)?;
    let mut out = match base_agent(agent) {
        "codex" => vec![
            "exec".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
        ],
        "claude" => {
            let mut argv = vec!["-p".to_string(), brief.to_string()];
            if options.mode.is_none() {
                argv.push("--permission-mode".to_string());
                argv.push("acceptEdits".to_string());
            }
            // The worker reports back with `omnix handoff report`; allow only that Bash call.
            argv.push("--allowedTools".to_string());
            argv.push("Bash(omnix handoff report *)".to_string());
            argv
        }
        "antigravity" => vec![
            "-p".to_string(),
            brief.to_string(),
            "--dangerously-skip-permissions".to_string(),
            "--output-format".to_string(),
            "text".to_string(),
        ],
        "opencode" | "opencode2" => vec!["run".to_string(), brief.to_string()],
        other => bail!("{other} has no verified headless mode; open a pane instead"),
    };
    out.extend(extra);
    if base_agent(agent) == "codex" {
        out.push(brief.to_string());
    }
    Ok(out)
}

/// OpenCode picks its model from config rather than a flag. Setting the
/// content config for one pane keeps the user's global config untouched while
/// still pinning the model the project team assigned.
pub fn model_env(agent: &str, model: Option<&str>) -> Vec<(String, String)> {
    let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) else {
        return Vec::new();
    };
    if base_agent(agent) != "opencode" {
        return Vec::new();
    }
    vec![(
        "OPENCODE_CONFIG_CONTENT".into(),
        serde_json::json!({ "model": model }).to_string(),
    )]
}

/// What `omnix doctor` prints. A desktop launch can miss a binary that is
/// installed for the login shell, so show the resolved path, not just a bool.
pub fn version(agent: &str) -> Option<String> {
    let path = resolve(agent)?;
    for flag in ["--version", "-V"] {
        let out = Command::new(&path).arg(flag).output().ok()?;
        if !out.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return Some(line.chars().take(120).collect());
        }
    }
    Some("installed".into())
}


/// Spawn without touching a shell, so session ids cannot inject commands.
pub fn spawn(agent: &str, session_id: Option<&str>, cwd: &str) -> Result<Child> {
    let base = launch_cmd(agent).ok_or_else(|| anyhow::anyhow!("unknown agent '{agent}'"))?;
    let mut cmd = Command::new(base);
    for (key, value) in config_for(&crate::root(), agent).env_list() {
        cmd.env(key, value);
    }
    for (key, value) in prepare_profile(&crate::root(), agent)? {
        cmd.env(key, value);
    }
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
    fn parses_profile_suffix_and_rejects_bad_ids() {
        assert_eq!(
            split_agent("codex:akun2"),
            Some(("codex", Some("akun2")))
        );
        assert_eq!(split_agent("codex"), Some(("codex", None)));
        assert!(split_agent("codex:").is_none());
        assert!(split_agent("codex:Akun").is_none());
        assert!(split_agent("codex:a/b").is_none());
        assert!(split_agent("codex:-bad").is_none());
        assert!(split_agent("nope:akun2").is_none());
        assert_eq!(launch_cmd("codex:akun2"), Some("codex"));
        assert_eq!(
            resume_args("codex:akun2", "s1").unwrap(),
            vec!["resume", "s1"]
        );
    }

    #[test]
    fn profiles_inherit_base_args_env_and_share_defaults() {
        let root = std::env::temp_dir().join(format!("omnix-profile-cfg-{}", std::process::id()));
        let dir = root.join(".omnix");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agents.json"),
            r#"{"codex":{"args":"--fast","env":"A=1"},"codex:akun2":{"args":"-m gpt-5","env":"B=2"}}"#,
        )
        .unwrap();
        let cfg = config_for(&root, "codex:akun2");
        assert_eq!(cfg.arg_list(), vec!["--fast", "-m", "gpt-5"]);
        assert!(cfg.env_list().contains(&("A".into(), "1".into())));
        assert!(cfg.env_list().contains(&("B".into(), "2".into())));
        assert!(cfg.enabled);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn claude_profile_gets_env_and_rule_symlinks() {
        let base = std::env::temp_dir().join(format!("omnix-profile-link-{}", std::process::id()));
        let home = base.join("home");
        let root = base.join("repo");
        std::fs::create_dir_all(home.join(".claude").join("skills")).unwrap();
        std::fs::create_dir_all(root.join(".omnix")).unwrap();
        std::fs::write(home.join(".claude").join("CLAUDE.md"), "rules\n").unwrap();
        std::fs::write(
            root.join(".omnix").join("agents.json"),
            r#"{"claude:akun2":{"label":"Akun 2"}}"#,
        )
        .unwrap();
        let env = prepare_profile_at(&home, &root, "claude:akun2").unwrap();
        let dir = profile_dir_at(&home, &root, "claude:akun2").unwrap();
        assert_eq!(
            env,
            vec![(
                "CLAUDE_CONFIG_DIR".to_string(),
                dir.to_string_lossy().into_owned()
            )]
        );
        assert!(dir.join("projects").is_dir());
        #[cfg(unix)]
        {
            let skills = std::fs::read_link(dir.join("skills")).unwrap();
            assert_eq!(skills, home.join(".claude").join("skills"));
            let rules = std::fs::read_link(dir.join("CLAUDE.md")).unwrap();
            assert_eq!(rules, home.join(".claude").join("CLAUDE.md"));
        }
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn profile_requires_a_supported_isolation_var() {
        let base = std::env::temp_dir().join(format!("omnix-profile-agy-{}", std::process::id()));
        std::fs::create_dir_all(base.join(".omnix")).unwrap();
        assert!(prepare_profile_at(&base, &base, "antigravity:akun2")
            .unwrap()
            .is_empty());
        assert!(profile_dir_at(&base, &base, "antigravity:akun2").is_none());
        std::fs::remove_dir_all(&base).ok();
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
            ..Default::default()
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

    #[test]
    fn session_ids_are_generated_in_process_as_uuid_v4() {
        let first = new_session_id();
        let second = new_session_id();
        assert_ne!(first, second);
        let parts: Vec<&str> = first.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(first.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_eq!(&first[14..15], "4", "version: {first}");
        assert!(
            matches!(&first[19..20], "8" | "9" | "a" | "b"),
            "variant: {first}"
        );
        assert_eq!(
            new_session_args("claude", &first),
            vec!["--session-id".to_string(), first.clone()]
        );
        assert!(new_session_args("codex", &first).is_empty());
        assert_eq!(
            extract_session_id("session id: abc-123").as_deref(),
            Some("abc-123")
        );
        assert!(extract_session_id("no id here").is_none());
    }

    #[test]
    fn extra_args_follow_each_cli_table() {
        let opts = LaunchOptions {
            model: Some("gpt-5".into()),
            effort: Some("high".into()),
            mode: None,
            prompt: None,
        };
        assert_eq!(
            extra_args("codex", &opts).unwrap(),
            vec!["-m", "gpt-5", "-c", "model_reasoning_effort=high"]
        );
        let antigravity = LaunchOptions {
            model: Some("gpt-5".into()),
            effort: Some("high".into()),
            mode: Some("edit".into()),
            prompt: None,
        };
        assert_eq!(
            extra_args("antigravity", &antigravity).unwrap(),
            vec!["--model", "gpt-5", "--effort", "high", "--mode", "accept-edits"]
        );
        let agy_full = LaunchOptions {
            mode: Some("full".into()),
            ..Default::default()
        };
        assert_eq!(
            extra_args("antigravity", &agy_full).unwrap(),
            vec!["--mode", "auto"]
        );
        let claude = LaunchOptions {
            model: Some("gpt-5".into()),
            ..Default::default()
        };
        assert_eq!(
            extra_args("claude", &claude).unwrap(),
            vec!["--model", "gpt-5"]
        );
        assert_eq!(
            extra_args("claude", &opts).unwrap(),
            vec!["--model", "gpt-5", "--effort", "high"]
        );
        let claude_mode = LaunchOptions {
            effort: Some("medium".into()),
            mode: Some("edit".into()),
            ..Default::default()
        };
        assert_eq!(
            extra_args("claude", &claude_mode).unwrap(),
            vec!["--effort", "medium", "--permission-mode", "acceptEdits"]
        );
        assert_eq!(
            extra_args(
                "claude",
                &LaunchOptions {
                    mode: Some("full".into()),
                    ..Default::default()
                }
            )
            .unwrap(),
            vec!["--permission-mode", "bypassPermissions"]
        );
        assert!(
            extra_args("opencode", &antigravity).is_err(),
            "opencode has no mode flag"
        );
    }

    #[test]
    fn headless_args_use_each_cli_print_mode() {
        let opts = LaunchOptions::default();
        assert_eq!(
            headless_args("codex", &opts, "do it").unwrap(),
            vec!["exec", "--dangerously-bypass-approvals-and-sandbox", "do it"]
        );
        assert_eq!(
            headless_args("antigravity", &opts, "do it").unwrap(),
            vec!["-p", "do it", "--dangerously-skip-permissions", "--output-format", "text"]
        );
        assert_eq!(
            headless_args("opencode", &opts, "do it").unwrap(),
            vec!["run", "do it"]
        );
        assert_eq!(
            headless_args("claude", &opts, "do it").unwrap(),
            vec![
                "-p",
                "do it",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "Bash(omnix handoff report *)",
            ]
        );
        assert!(headless_args("omp", &opts, "do it").is_err());
        assert!(headless_args("codex", &opts, " ").is_err());
    }

    #[test]
    fn opencode_takes_the_first_turn_as_argv() {
        let opts = LaunchOptions {
            prompt: Some("Plan the retry cap".into()),
            ..Default::default()
        };
        assert_eq!(
            extra_args("opencode", &opts).unwrap(),
            vec!["--prompt", "Plan the retry cap"]
        );
        assert_eq!(
            extra_args("opencode2", &opts).unwrap(),
            vec!["--prompt", "Plan the retry cap"]
        );
        assert!(extra_args("codex", &opts).is_err());
        let long = LaunchOptions {
            prompt: Some("x".repeat(20001)),
            ..Default::default()
        };
        assert!(extra_args("opencode", &long).is_err());
    }

    #[test]
    fn extra_args_reject_flag_shaped_values() {
        let bad = LaunchOptions {
            model: Some("--sandbox".into()),
            ..Default::default()
        };
        assert!(extra_args("codex", &bad).is_err());
        let bad = LaunchOptions {
            effort: Some("turbo".into()),
            ..Default::default()
        };
        assert!(extra_args("codex", &bad).is_err());
    }

    #[test]
    fn opencode_model_is_passed_through_the_content_config() {
        assert_eq!(
            model_env("opencode", Some("9router/oc/model")),
            vec![(
                "OPENCODE_CONFIG_CONTENT".to_string(),
                r#"{"model":"9router/oc/model"}"#.to_string()
            )]
        );
        assert!(model_env("claude", Some("sonnet")).is_empty());
        assert!(model_env("opencode", None).is_empty());
        assert!(model_env("opencode", Some("  ")).is_empty());
    }
}
