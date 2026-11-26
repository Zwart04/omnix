use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One owner-configured fallback. `command` is typed into the live pane when
/// quota runs out; `agent` opens a fresh pane with the same brief. Neither
/// path kills the exhausted process, so its transcript stays readable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Fallback {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub auto: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

pub fn path(root: &Path) -> PathBuf {
    root.join(".omnix").join("fallbacks.json")
}

pub fn load(root: &Path) -> BTreeMap<String, Fallback> {
    let Ok(text) = std::fs::read_to_string(path(root)) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn get(root: &Path, agent: &str) -> Option<Fallback> {
    load(root)
        .remove(agent)
        .filter(|fallback| fallback.enabled)
}

pub fn set(root: &Path, agent: &str, fallback: Fallback) -> Result<()> {
    if !crate::agent::known(agent) {
        anyhow::bail!("unknown agent '{agent}'");
    }
    if let Some(command) = fallback.command.as_deref() {
        if command.trim().is_empty() || command.contains('\n') || command.len() > 2000 {
            anyhow::bail!("fallback command must be one non-empty line (max 2000 chars)");
        }
    }
    if fallback.agent.is_empty() && fallback.command.is_none() {
        anyhow::bail!("fallback needs an agent or a command");
    }
    if !fallback.agent.is_empty() && !crate::agent::known(&fallback.agent) {
        anyhow::bail!("unknown fallback agent '{}'", fallback.agent);
    }
    let mut map = load(root);
    map.insert(agent.to_string(), fallback);
    let file = path(root);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&map)?;
    std::fs::write(&file, text).with_context(|| format!("write {}", file.display()))?;
    Ok(())
}

/// Drop the stored fallback for one agent. Missing keys are not an error.
pub fn remove(root: &Path, agent: &str) -> Result<()> {
    let mut map = load(root);
    if map.remove(agent).is_none() {
        return Ok(());
    }
    let file = path(root);
    std::fs::write(&file, serde_json::to_string_pretty(&map)?)
        .with_context(|| format!("write {}", file.display()))?;
    Ok(())
}

/// Candidate agents for the UI picker: the configured fallback first, then
/// installed CLIs that are not the exhausted one.
pub fn candidates(root: &Path, exhausted: &str) -> Vec<Value> {
    let configured = get(root, exhausted);
    let mut rows = Vec::new();
    if let Some(fallback) = configured {
        if !fallback.agent.is_empty() {
            rows.push(json!({
                "agent": fallback.agent,
                "configured": true,
                "command": fallback.command,
            }));
        }
    }
    for agent in crate::agent::AGENTS {
        let name = *agent;
        if name == exhausted || rows.iter().any(|row| row["agent"] == name) {
            continue;
        }
        if crate::agent::resolve(name).is_some() {
            rows.push(json!({ "agent": name, "configured": false }));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omnix-fallback-{tag}-{}", std::process::id()))
    }

    #[test]
    fn stores_and_reads_one_fallback_per_agent() {
        let root = temp_root("store");
        std::fs::create_dir_all(&root).unwrap();
        set(
            &root,
            "codex",
            Fallback {
                agent: "claude".into(),
                command: None,
                auto: true,
                enabled: true,
            },
        )
        .unwrap();
        let got = get(&root, "codex").expect("fallback");
        assert_eq!(got.agent, "claude");
        assert!(got.auto);
        assert!(get(&root, "claude").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_unknown_agents_and_multiline_commands() {
        let root = temp_root("reject");
        std::fs::create_dir_all(&root).unwrap();
        assert!(set(&root, "nope", Fallback::default()).is_err());
        assert!(
            set(
                &root,
                "codex",
                Fallback {
                    agent: "claude".into(),
                    command: Some("a\nb".into()),
                    auto: false,
                    enabled: true,
                }
            )
            .is_err()
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
