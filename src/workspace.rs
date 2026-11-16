use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub workspaces: BTreeMap<String, Workspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub sleeping: bool,
    #[serde(default)]
    pub slept_at: Option<u64>,
    #[serde(default)]
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub pane_key: String,
    pub agent: String,
    pub session_id: String,
    pub cwd: String,
    pub resume_command: String,
    pub saved_at: u64,
}

fn state_path(root: &Path) -> PathBuf {
    root.join(".perch").join("workspaces.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load(root: &Path) -> Result<State> {
    let path = state_path(root);
    if !path.exists() {
        return Ok(State::default());
    }
    let raw = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save(root: &Path, state: &State) -> Result<()> {
    let path = state_path(root);
    fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn current_branch(path: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", path.to_str()?, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let branch = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!branch.is_empty() && branch != "HEAD").then_some(branch)
}

fn canonical(path: &Path) -> Result<PathBuf> {
    Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

pub fn get_or_create<'a>(state: &'a mut State, path: &Path) -> Result<&'a mut Workspace> {
    let path = canonical(path)?;
    let key = path.to_string_lossy().to_string();
    Ok(match state.workspaces.entry(key.clone()) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => entry.insert(Workspace {
            path: key,
            branch: current_branch(&path),
            sleeping: false,
            slept_at: None,
            sessions: Vec::new(),
        }),
    })
}

pub fn track(
    root: &Path,
    workspace: &Path,
    agent: &str,
    session_id: &str,
    cwd: &Path,
    pane_key: &str,
) -> Result<Session> {
    let resume_command = super::agent::resume_command(agent, session_id)?;
    let mut state = load(root)?;
    let ws = get_or_create(&mut state, workspace)?;
    let record = Session {
        pane_key: pane_key.into(),
        agent: agent.into(),
        session_id: session_id.into(),
        cwd: cwd.to_string_lossy().to_string(),
        resume_command,
        saved_at: now(),
    };
    ws.sessions.retain(|s| s.pane_key != pane_key);
    ws.sessions.push(record.clone());
    save(root, &state)?;
    Ok(record)
}

pub fn sleep(root: &Path, workspace: &Path) -> Result<Workspace> {
    let mut state = load(root)?;
    let ws = get_or_create(&mut state, workspace)?;
    ws.sleeping = true;
    ws.slept_at = Some(now());
    let out = ws.clone();
    save(root, &state)?;
    Ok(out)
}

/// Human-readable session list, used by the CLI only.
pub fn describe(ws: &Workspace) {
    for session in &ws.sessions {
        println!(
            "{}  {}  {}\n  cd {}\n  {}",
            session.pane_key,
            session.agent,
            session.session_id,
            session.cwd,
            session.resume_command
        );
    }
}

pub fn wake(root: &Path, workspace: &Path, run: bool) -> Result<Workspace> {
    let mut state = load(root)?;
    let ws = get_or_create(&mut state, workspace)?;
    if ws.sessions.is_empty() {
        bail!("no agent sessions recorded for {}", ws.path);
    }
    if run {
        for session in &ws.sessions {
            let child =
                super::agent::spawn(&session.agent, Some(&session.session_id), &session.cwd)
                    .with_context(|| format!("resume {} in {}", session.agent, session.cwd))?;
            println!("  -> pid {}", child.id());
            drop(child);
        }
    }
    ws.sleeping = false;
    ws.slept_at = None;
    let out = ws.clone();
    save(root, &state)?;
    Ok(out)
}

pub fn list(root: &Path) -> Result<Vec<Workspace>> {
    Ok(load(root)?.workspaces.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "perch-ws-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sleep_then_resume_keeps_session_and_clears_sleep() {
        let dir = temp_dir();
        let ws = dir.join("repo");
        fs::create_dir_all(&ws).unwrap();
        let record = track(&dir, &ws, "codex", "sess-9", &ws, "tab:1").unwrap();
        assert_eq!(record.resume_command, "codex resume sess-9");

        let slept = sleep(&dir, &ws).unwrap();
        assert!(slept.sleeping);
        assert_eq!(slept.sessions.len(), 1);

        let woke = wake(&dir, &ws, false).unwrap();
        assert!(!woke.sleeping);
        assert_eq!(woke.sessions[0].session_id, "sess-9");

        let raw = fs::read_to_string(dir.join(".perch/workspaces.json")).unwrap();
        assert!(raw.contains("sess-9"));
    }

    #[test]
    fn state_file_survives_hand_edit_missing_fields() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join(".perch")).unwrap();
        let mut f = fs::File::create(dir.join(".perch/workspaces.json")).unwrap();
        writeln!(f, r#"{{"workspaces":{{"/x":{{"path":"/x"}}}}}}"#).unwrap();
        let state = load(&dir).unwrap();
        assert_eq!(state.workspaces.len(), 1);
        assert!(state.workspaces["/x"].sessions.is_empty());
    }
}
