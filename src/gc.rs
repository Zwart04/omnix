//! Deliberately invoked cleanup. Nothing here runs on a timer: `omnix gc` or
//! the GUI preview button calls `run` once and exits.

use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::handoff;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn merged(repo: &Path, branch: &str, base: &str) -> bool {
    git(repo, &["branch", "--merged", base])
        .map(|text| {
            text.lines()
                .map(|line| line.trim().trim_start_matches("* ").trim())
                .any(|line| line == branch)
        })
        .unwrap_or(false)
}

fn archive_old(record: &mut handoff::Handoff, cutoff: u64, actions: &mut Vec<String>) -> bool {
    let mut changed = false;
    for kind in ["brief", "report"] {
        let path = match kind {
            "brief" => PathBuf::from(&record.brief),
            _ => match &record.report {
                Some(path) => PathBuf::from(path),
                None => continue,
            },
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(now());
        if modified > cutoff {
            continue;
        }
        let archive = PathBuf::from(&record.repo).join(".omnix").join("archive");
        let target = archive.join(path.file_name().unwrap_or_default());
        if std::fs::create_dir_all(&archive).is_err() {
            continue;
        }
        if std::fs::rename(&path, &target).is_err() {
            continue;
        }
        actions.push(format!("archive {}", target.display()));
        if kind == "brief" {
            record.brief = target.to_string_lossy().into_owned();
        } else {
            record.report = Some(target.to_string_lossy().into_owned());
        }
        changed = true;
    }
    changed
}

/// `dry` returns the same action list without touching disk, the process
/// table, or the git index.
pub fn run(_root: &Path, dry: bool) -> Result<Value> {
    let mut actions = Vec::new();
    let records = handoff::list_all();

    for mut record in records.iter().cloned() {
        if record.closed {
            let cutoff = now().saturating_sub(30 * 86_400);
            if archive_old(&mut record, cutoff, &mut actions) && !dry {
                handoff::save(&record)?;
            }
            continue;
        }
        if handoff::prune_orphans(std::slice::from_ref(&record)).len() == 1 {
            if dry {
                actions.push(format!("forget {} (repo is gone)", record.name));
            } else {
                handoff::forget(Path::new(&handoff::record_path(&record)))?;
                actions.push(format!("forget {} (repo is gone)", record.name));
            }
            continue;
        }
        let repo = PathBuf::from(&record.repo);
        if merged(&repo, &record.branch, &record.base) {
            if dry {
                actions.push(format!(
                    "close {} ({} is merged)",
                    record.name, record.branch
                ));
            } else {
                match handoff::close(&record.name) {
                    Ok(done) => actions.push(format!("closed {}", done.name)),
                    Err(err) => actions.push(format!("skip {}: {err:#}", record.name)),
                }
            }
        }
    }

    // The browser is shared, so only stop it when no Omnix window owns it.
    if crate::control::read_control(&crate::control::runtime_dir()).is_none() {
        if let Some((pid, _port)) = crate::browser::running() {
            if dry {
                actions.push(format!("stop Lightpanda pid {pid}"));
            } else if crate::browser::stop(None, true).is_ok() {
                actions.push(format!("stopped Lightpanda pid {pid}"));
            }
        }
    }

    actions.extend(crate::preview::prune(_root, dry));

    if !dry {
        let db = _root.join(".omnix").join("history.sqlite");
        if db.exists() {
            if let Ok(mut conn) = crate::history::open(&db) {
                if let Ok(report) = crate::sources::scan(&mut conn, &_root) {
                    if report.pruned > 0 {
                        actions.push(format!("pruned {} missing transcript(s)", report.pruned));
                    }
                }
            }
        }
    }

    Ok(json!({ "dryRun": dry, "actions": actions }))
}
