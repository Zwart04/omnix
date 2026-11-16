use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub mode: String,
    pub preferred: String,
    pub reason: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub worktree: String,
    pub branch: String,
    pub base: String,
    pub agent: String,
    pub brief: String,
    pub prompt_sha256: String,
    pub created_at: u64,
    /// Orca vocabulary: journaled | handed-to-terminal | not-delivered | unconfirmed
    pub outcome: String,
    pub receipt: Receipt,
    #[serde(default)]
    pub warning: Option<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("spawn git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn slug(name: &str) -> Result<String> {
    let s = name.trim().to_ascii_lowercase();
    if s.is_empty()
        || s.len() > 48
        || !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || s.starts_with('-')
        || s.ends_with('-')
    {
        bail!("name must be 1-48 chars of a-z, 0-9, '-'");
    }
    Ok(s)
}

fn write_record(dir: &Path, record: &Handoff) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", record.id));
    fs::write(&path, serde_json::to_vec_pretty(record)?)?;
    Ok(path)
}

/// Mirrors Orca: create worktree, write the brief durably, then deliver.
/// The prompt outcome is one of journaled | handed-to-terminal | not-delivered | unconfirmed.
pub fn create(
    root: &Path,
    name: &str,
    agent: &str,
    prompt: &str,
    base: Option<&str>,
    run: bool,
) -> Result<(Handoff, PathBuf)> {
    let name = slug(name)?;
    if super::agent::launch_cmd(agent).is_none() {
        bail!(
            "unknown agent '{agent}' (known: {})",
            super::agent::AGENTS.join(", ")
        );
    }
    let repo = git(root, &["rev-parse", "--show-toplevel"])?;
    let repo = PathBuf::from(repo);
    let base = match base {
        Some(base) => base.to_string(),
        None => git(&repo, &["rev-parse", "--short", "HEAD"])?,
    };
    let branch = format!("perch/{name}");
    let worktree = repo.join(".perch").join("worktrees").join(&name);

    if worktree.exists() {
        bail!("worktree already exists: {}", worktree.display());
    }
    fs::create_dir_all(worktree.parent().unwrap())?;

    let mut add = vec!["worktree", "add", "-b", branch.as_str()];
    let path = worktree.to_string_lossy().to_string();
    add.push(&path);
    if base != "HEAD" {
        add.push(&base);
    }
    if let Err(first) = git(&repo, &add) {
        // Branch may already exist; attach to it instead of creating another.
        git(&repo, &["worktree", "add", &path, &branch])
            .with_context(|| format!("create worktree failed first as: {first}"))?;
    }

    let id = format!("{}-{name}", now());
    let handoff_dir = repo.join(".perch").join("handoffs");
    fs::create_dir_all(&handoff_dir)?;
    let brief = handoff_dir.join(format!("{id}.md"));
    let brief_body = format!(
        "# Handoff: {name}\n\n- Agent: {agent}\n- Repo: {}\n- Branch: {branch}\n- Base: {base}\n- Created: {}\n\n## Task\n\n{prompt}\n",
        repo.display(),
        id
    );
    fs::write(&brief, &brief_body)?;

    let mut record = Handoff {
        id: id.clone(),
        name,
        repo: repo.to_string_lossy().to_string(),
        worktree: worktree.to_string_lossy().to_string(),
        branch,
        base,
        agent: agent.into(),
        brief: brief.to_string_lossy().to_string(),
        prompt_sha256: format!("{:x}", Sha256::digest(prompt.as_bytes())),
        created_at: now(),
        outcome: "not-delivered".into(),
        receipt: Receipt {
            mode: "terminal".into(),
            preferred: "terminal".into(),
            reason: "cli-handoff".into(),
            detail: "dry run: worktree and brief written; prompt not injected".into(),
        },
        warning: None,
    };
    let record_path = write_record(&handoff_dir, &record)?;

    if run {
        let child = super::agent::spawn(agent, None, &worktree.to_string_lossy());
        match child {
            Ok(child) => {
                record.outcome = "unconfirmed".into();
                record.receipt.detail =
                    "agent started; brief saved but PTY prompt injection is not wired yet".into();
                record.warning = Some(format!(
                    "agent pid {}; deliver {} into its terminal",
                    child.id(),
                    brief.display()
                ));
                drop(child);
            }
            Err(e) => {
                record.outcome = "not-delivered".into();
                record.receipt.detail = format!("agent launch failed: {e:#}");
                record.warning = Some(e.to_string());
            }
        }
        write_record(&handoff_dir, &record)?;
    }

    Ok((record, record_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert_eq!(slug("fix-login").unwrap(), "fix-login");
        assert!(slug("Fix Login").is_err());
        assert!(slug("-bad").is_err());
        assert!(slug("").is_err());
    }

    #[test]
    fn dry_run_creates_worktree_brief_and_record() {
        let dir =
            std::env::temp_dir().join(format!("perch-handoff-{}-{}", std::process::id(), now()));
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).unwrap();
        fs::write(dir.join("README.md"), "x\n").unwrap();
        git(&dir, &["add", "."]).unwrap();
        let out = Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=T",
                "commit",
                "-qm",
                "init",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());

        let (record, path) =
            create(&dir, "task-one", "codex", "do the thing", None, false).unwrap();
        assert_eq!(record.outcome, "not-delivered");
        assert!(Path::new(&record.worktree).exists());
        assert!(Path::new(&record.brief).exists());
        assert!(path.exists());
        assert!(
            fs::read_to_string(&record.brief)
                .unwrap()
                .contains("do the thing")
        );
    }
}
