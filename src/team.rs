//! Per-project team and orchestrator settings.
//!
//! A project may allow a fixed set of agents/models. When the list is not
//! empty, the GUI handoff picker only offers those members and the host
//! rejects anything else, so a stale page cannot start an unlisted agent.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_MEMBERS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub agent: String,
    pub model: String,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Team {
    /// `manual` only restricts handoffs; `auto` also starts the team.
    pub mode: String,
    /// Member key (`agent|model`) that coordinates the workers.
    pub orchestrator: String,
    /// What to do when a worker hits quota: `orchestrator`, `fallback`, `wait`, or `stop`.
    pub on_quota: String,
    pub members: Vec<Member>,
}

impl Default for Team {
    fn default() -> Self {
        Self {
            mode: "manual".into(),
            orchestrator: String::new(),
            on_quota: "orchestrator".into(),
            members: Vec::new(),
        }
    }
}

/// Stable key shared with the UI. The empty model means "agent default".
pub fn member_key(member: &Member) -> String {
    let agent = member.agent.trim();
    let model = member.model.trim();
    if model.is_empty() {
        agent.to_string()
    } else {
        format!("{agent}|{model}")
    }
}

/// Walk up from a file or folder until the project root (`.git` or an
/// existing team file) is found. Worktrees live under the repo, so this keeps
/// one team file for the whole project.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut path = start.to_path_buf();
    if path.is_file() {
        path.pop();
    }
    if path.as_os_str().is_empty() {
        path = PathBuf::from(".");
    }
    if let Ok(canonical) = path.canonicalize() {
        path = canonical;
    }
    loop {
        if path.join(".git").exists() || path.join(".omnix").join("team.json").is_file() {
            return Some(path);
        }
        if !path.pop() {
            return None;
        }
    }
}

pub fn path_at(project: &Path) -> PathBuf {
    project.join(".omnix").join("team.json")
}

/// A missing file is an empty team: every agent is still allowed.
pub fn load_at(project: &Path) -> Result<Team> {
    let file = path_at(project);
    if !file.is_file() {
        return Ok(Team::default());
    }
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("read team file {}", file.display()))?;
    let team: Team = serde_json::from_str(&text)
        .with_context(|| format!("parse team file {}", file.display()))?;
    normalize(team)
}

pub fn save_at(project: &Path, team: Team) -> Result<Team> {
    let team = normalize(team)?;
    let dir = project.join(".omnix");
    std::fs::create_dir_all(&dir)?;
    let file = path_at(project);
    let tmp = dir.join(format!("team.json.tmp-{}", std::process::id()));
    let text = serde_json::to_string_pretty(&team)?;
    std::fs::write(&tmp, format!("{text}\n"))?;
    std::fs::rename(&tmp, &file)?;
    Ok(team)
}

pub fn normalize(mut team: Team) -> Result<Team> {
    team.mode = team.mode.trim().to_ascii_lowercase();
    if team.mode.is_empty() {
        team.mode = "manual".into();
    }
    if team.mode != "manual" && team.mode != "auto" {
        bail!("team mode must be manual or auto");
    }
    team.on_quota = team.on_quota.trim().to_ascii_lowercase();
    if team.on_quota.is_empty() {
        team.on_quota = "orchestrator".into();
    }
    if !matches!(
        team.on_quota.as_str(),
        "orchestrator" | "fallback" | "wait" | "stop"
    ) {
        bail!("team on_quota must be orchestrator, fallback, wait, or stop");
    }
    if team.members.len() > MAX_MEMBERS {
        bail!("a team can have at most {MAX_MEMBERS} members");
    }
    let mut seen = std::collections::HashSet::new();
    for member in &mut team.members {
        member.agent = member.agent.trim().to_string();
        member.model = member.model.trim().to_string();
        member.role = member.role.trim().to_string();
        if member.role.len() > 120 {
            bail!("team role must be at most 120 characters");
        }
        if member.agent.is_empty() {
            bail!("every team member needs an agent");
        }
        if crate::agent::launch_cmd(&member.agent).is_none() {
            bail!("unknown team agent '{}'", member.agent);
        }
        let options = crate::agent::LaunchOptions {
            model: if member.model.is_empty() {
                None
            } else {
                Some(member.model.clone())
            },
            ..Default::default()
        };
        crate::agent::extra_args(&member.agent, &options)?;
        let key = member_key(member);
        if !seen.insert(key.clone()) {
            bail!("team member '{key}' is listed twice");
        }
    }
    team.orchestrator = team.orchestrator.trim().to_string();
    if team.members.is_empty() {
        team.orchestrator.clear();
        return Ok(team);
    }
    if team.orchestrator.is_empty() {
        if team.mode == "auto" {
            bail!("choose an orchestrator for the team");
        }
        return Ok(team);
    }
    if !team
        .members
        .iter()
        .any(|m| member_key(m) == team.orchestrator)
    {
        bail!("the orchestrator must be one of the listed team members");
    }
    Ok(team)
}

/// Map the team-level quota policy to a per-handoff `--on-quota` spec.
///
/// `wait` and `stop` pass straight through. `fallback` walks the other team
/// members in order, so an exhausted worker hands its worktree to a teammate.
/// `orchestrator` returns `None`: the pane watcher already notifies the
/// orchestrator and saves a WIP commit on its own.
pub fn quota_spec(team: &Team, current: &Member) -> Option<String> {
    match team.on_quota.as_str() {
        "wait" => Some("wait".into()),
        "stop" => Some("stop".into()),
        "fallback" => {
            let current_key = member_key(current);
            let list = team
                .members
                .iter()
                .filter(|m| member_key(m) != current_key)
                .map(|m| {
                    if m.model.is_empty() {
                        m.agent.clone()
                    } else {
                        format!("{}:{}", m.agent, m.model)
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            if list.is_empty() {
                None
            } else {
                Some(format!("fallback={list}"))
            }
        }
        _ => None,
    }
}

/// Human label for a preset role. Unknown roles are treated as custom text.
pub fn role_label(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "planner" => "Planner",
        "implementer" => "Implementer",
        "reviewer" => "Reviewer",
        "tester" => "Tester",
        "researcher" => "Researcher",
        "debugger" => "Debugger",
        "documenter" => "Documenter",
        "orchestrator" | "coordinator" => "Coordinator",
        _ => "Custom",
    }
}

/// Prompt text for a preset role. The GUI shows the same strings, so the
/// template the user picks is the one the agent receives.
pub fn role_brief(role: &str) -> Option<&'static str> {
    match role.trim().to_ascii_lowercase().as_str() {
        "planner" => Some(
            "Break the task into small steps, list the files and risks, and hand each step to a worker.",
        ),
        "implementer" => {
            Some("Implement the assigned change, run the tests, and report the diff and results.")
        }
        "reviewer" => Some(
            "Review the diff for bugs, regressions, and missing tests. Do not rewrite unless asked.",
        ),
        "tester" => Some("Write and run tests, reproduce failures, and report coverage gaps."),
        "researcher" => {
            Some("Investigate APIs and docs, then report findings with file references.")
        }
        "debugger" => {
            Some("Reproduce the failure, find the root cause, and propose the smallest fix.")
        }
        "documenter" => Some("Update the README and docs, and keep examples accurate."),
        "orchestrator" | "coordinator" => Some(
            "Coordinate the team: split the task, send one assignment per worker, wait for results, and summarize.",
        ),
        _ => None,
    }
}

/// One line for the prompt. Presets expand to their template; custom text is
/// kept verbatim so a team saved by an older build still works.
pub fn role_note(role: &str, fallback: &str) -> String {
    let text = role.trim();
    let key = if text.is_empty() { fallback } else { text };
    match role_brief(key) {
        Some(brief) => format!("{}: {brief}", role_label(key)),
        None => format!("Custom role: {text}"),
    }
}

/// Resolve a requested agent/model against the allowlist. No members means no
/// restriction, which keeps projects that never configured a team working.
pub fn resolve<'a>(team: &'a Team, agent: &str, model: &str) -> Result<Option<&'a Member>> {
    if team.members.is_empty() {
        return Ok(None);
    }
    let agent = agent.trim();
    let model = model.trim();
    for member in &team.members {
        if member.agent != agent {
            continue;
        }
        if model.is_empty() || member.model == model {
            return Ok(Some(member));
        }
    }
    let label = if model.is_empty() {
        agent.to_string()
    } else {
        format!("{agent} ({model})")
    };
    bail!("{label} is not in this project's team allowlist");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnix-team-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn member(agent: &str, model: &str) -> Member {
        Member {
            agent: agent.into(),
            model: model.into(),
            role: String::new(),
        }
    }

    #[test]
    fn saves_and_loads_a_team_under_the_project() {
        let dir = temp_dir("roundtrip");
        let team = Team {
            mode: "auto".into(),
            orchestrator: member_key(&member("opencode", "9router/model")),
            on_quota: "orchestrator".into(),
            members: vec![
                member("opencode", "9router/model"),
                member("codex", "gpt-5"),
            ],
        };
        let saved = save_at(&dir, team.clone()).unwrap();
        assert_eq!(saved, team);
        assert_eq!(load_at(&dir).unwrap(), team);
        assert_eq!(find_root(&dir), Some(dir.canonicalize().unwrap()));
    }

    #[test]
    fn missing_file_means_unrestricted() {
        let dir = temp_dir("missing");
        let team = load_at(&dir).unwrap();
        assert!(team.members.is_empty());
        assert!(resolve(&team, "claude", "sonnet").unwrap().is_none());
    }

    #[test]
    fn rejects_an_agent_outside_the_allowlist() {
        let team = normalize(Team {
            mode: "manual".into(),
            orchestrator: String::new(),
            on_quota: "orchestrator".into(),
            members: vec![member("opencode", "9router/model")],
        })
        .unwrap();
        assert!(
            resolve(&team, "opencode", "9router/model")
                .unwrap()
                .is_some()
        );
        assert!(resolve(&team, "opencode", "").unwrap().is_some());
        assert!(resolve(&team, "codex", "").is_err());
        assert!(resolve(&team, "opencode", "other/model").is_err());
    }

    #[test]
    fn auto_mode_requires_a_member_orchestrator() {
        let base = Team {
            mode: "auto".into(),
            orchestrator: String::new(),
            on_quota: "orchestrator".into(),
            members: vec![member("opencode", "")],
        };
        assert!(normalize(base.clone()).is_err());
        let mut bad = base;
        bad.orchestrator = "claude".into();
        assert!(normalize(bad).is_err());
    }

    #[test]
    fn duplicates_and_unknown_agents_are_rejected() {
        let dup = Team {
            mode: "manual".into(),
            orchestrator: String::new(),
            on_quota: "orchestrator".into(),
            members: vec![member("opencode", ""), member("opencode", "")],
        };
        assert!(normalize(dup).is_err());
        let unknown = Team {
            mode: "manual".into(),
            orchestrator: String::new(),
            on_quota: "orchestrator".into(),
            members: vec![member("not-an-agent", "")],
        };
        assert!(normalize(unknown).is_err());
    }

    #[test]
    fn role_templates_reach_the_prompt() {
        assert!(role_note("", "implementer").starts_with("Implementer:"));
        assert!(role_note("reviewer", "").contains("bugs"));
        assert!(role_note("orchestrator", "").starts_with("Coordinator:"));
        assert_eq!(role_note("audit the API", ""), "Custom role: audit the API");
    }

    #[test]
    fn quota_spec_maps_team_policy_to_handoff() {
        let team = Team {
            mode: "auto".into(),
            orchestrator: member_key(&member("opencode", "9router/model")),
            on_quota: "orchestrator".into(),
            members: vec![
                member("opencode", "9router/model"),
                member("codex", "gpt-5"),
                member("claude", ""),
            ],
        };
        // `orchestrator` is handled by the pane watcher; no handoff spec.
        assert_eq!(quota_spec(&team, &team.members[0]), None);

        let mut wait = team.clone();
        wait.on_quota = "wait".into();
        assert_eq!(quota_spec(&wait, &wait.members[0]), Some("wait".into()));

        let mut stop = team.clone();
        stop.on_quota = "stop".into();
        assert_eq!(quota_spec(&stop, &stop.members[0]), Some("stop".into()));

        let mut fallback = team.clone();
        fallback.on_quota = "fallback".into();
        assert_eq!(
            quota_spec(&fallback, &fallback.members[0]),
            Some("fallback=codex:gpt-5,claude".into())
        );
    }
}
