use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// One rate-limit window reported by an agent CLI.
#[derive(Debug, PartialEq)]
pub struct Window {
    pub kind: String,
    pub used_percent: f64,
    pub resets_in_seconds: i64,
    pub window_minutes: i64,
}

/// Pull one `payload.rate_limits` object out of a Codex rollout line.
pub fn parse_codex_line(line: &str) -> Option<(Vec<Window>, Option<String>)> {
    let value: Value = serde_json::from_str(line).ok()?;
    let limits = value.pointer("/payload/rate_limits")?;
    let mut windows = Vec::new();
    for kind in ["primary", "secondary"] {
        let Some(win) = limits.get(kind) else { continue };
        let Some(used) = win.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        windows.push(Window {
            kind: kind.to_string(),
            used_percent: used,
            resets_in_seconds: win
                .get("resets_in_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            window_minutes: win
                .get("window_minutes")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        });
    }
    if windows.is_empty() {
        return None;
    }
    let plan = limits
        .get("plan_type")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((windows, plan))
}

/// Newest rollout file under `~/.codex/sessions`, where Codex journals usage.
pub fn newest_codex_rollout(home: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, best: &mut Option<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(&path, best);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            let Ok(modified) = meta.modified() else { continue };
            if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                *best = Some((modified, path));
            }
        }
    }
    let mut best = None;
    walk(&home.join(".codex").join("sessions"), &mut best);
    best.map(|(_, path)| path)
}

/// Codex appends a full rate-limit snapshot on each token-count event, so the
/// last one in the newest rollout is the current state.
pub fn codex_quota(home: &Path) -> Option<Value> {
    use std::io::{Read, Seek, SeekFrom};
    let path = newest_codex_rollout(home)?;
    /* Rollouts grow to megabytes; the newest rate-limit snapshot is always in
       the tail, so read the last 256 KiB instead of the whole transcript. */
    let mut file = std::fs::File::open(&path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(256 * 1024);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    /* resets_in_seconds was true when the line was written; age it so a stale
       snapshot cannot claim a reset window that already passed. */
    let age = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for line in text.lines().rev() {
        if !line.contains("\"rate_limits\"") {
            continue;
        }
        let Some((windows, plan)) = parse_codex_line(line) else {
            continue;
        };
        return Some(json!({
            "agent": "codex",
            "plan": plan,
            "windows": windows.iter().map(|w| json!({
                "kind": w.kind,
                "usedPercent": w.used_percent,
                "resetsInSeconds": (w.resets_in_seconds - age).max(0),
                "windowMinutes": w.window_minutes,
            })).collect::<Vec<_>>(),
        }));
    }
    None
}

/// System memory in bytes. Linux reads /proc, macOS shells out to sysctl and
/// vm_stat, Windows reads the Win32 memory counters via PowerShell.
pub fn ram() -> Option<Value> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mut total = 0u64;
        let mut available = 0u64;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let key = parts.next()?;
            let value: u64 = parts.next()?.parse().ok()?;
            match key {
                "MemTotal:" => total = value * 1024,
                "MemAvailable:" => available = value * 1024,
                _ => {}
            }
        }
        if total == 0 {
            return None;
        }
        return Some(json!({ "usedBytes": total.saturating_sub(available), "totalBytes": total }));
    }
    #[cfg(target_os = "macos")]
    {
        let total = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())?;
        let vm = std::process::Command::new("vm_stat")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())?;
        let page = vm
            .lines()
            .find_map(|l| l.split("page size of ").nth(1))
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(4096);
        let pages = |label: &str| -> u64 {
            vm.lines()
                .find(|l| l.trim_start().starts_with(label))
                .and_then(|l| l.rsplit(':').next())
                .and_then(|s| s.trim().trim_end_matches('.').parse::<u64>().ok())
                .unwrap_or(0)
        };
        let free = (pages("Pages free") + pages("Pages inactive")) * page;
        return Some(json!({ "usedBytes": total.saturating_sub(free), "totalBytes": total }));
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$o=Get-CimInstance Win32_OperatingSystem; \"$($o.TotalVisibleMemorySize) $($o.FreePhysicalMemory)\"",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        let mut parts = text.split_whitespace();
        let total: u64 = parts.next()?.parse().ok()?;
        let free: u64 = parts.next()?.parse().ok()?;
        return Some(json!({
            "usedBytes": total.saturating_sub(free) * 1024,
            "totalBytes": total * 1024,
        }));
    }
    #[allow(unreachable_code)]
    None
}

pub fn snapshot(home: &Path) -> Value {
    let mut agents = Vec::new();
    if let Some(codex) = codex_quota(home) {
        agents.push(codex);
    }
    json!({ "ram": ram(), "agents": agents })
}

/// Ceiling for terminal and agent memory when `OMNIX_AI_RAM_MB` is unset:
/// a quarter of system RAM, so the status chip warns before the machine swaps.
pub fn default_budget() -> Option<u64> {
    ram()
        .and_then(|value| value.get("totalBytes").and_then(Value::as_u64))
        .map(|total| total / 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_codex_rate_limits() {
        let line = r#"{"timestamp":"t","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":100.0,"window_minutes":300,"resets_in_seconds":900000},"secondary":{"used_percent":11.0,"window_minutes":10080,"resets_in_seconds":4000},"plan_type":"free"}}}"#;
        let (windows, plan) = parse_codex_line(line).expect("rate limits present");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].kind, "primary");
        assert!((windows[0].used_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(windows[0].resets_in_seconds, 900000);
        assert_eq!(plan.as_deref(), Some("free"));
        assert!(parse_codex_line(r#"{"payload":{}}"#).is_none());
    }

    #[test]
    fn newest_rollout_wins() {
        let root = std::env::temp_dir().join(format!("omnix-usage-{}", std::process::id()));
        let dir = root.join(".codex/sessions/2026/11/18");
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("rollout-old.jsonl");
        let new = dir.join("rollout-new.jsonl");
        std::fs::write(&old, "{}\n").unwrap();
        std::fs::write(&new, "{\"payload\":{}}\n").unwrap();
        let found = newest_codex_rollout(&root).expect("rollout");
        assert_eq!(found, new);
        let mut f = std::fs::OpenOptions::new().append(true).open(&new).unwrap();
        /* quota lives in the tail, not in a multi-megabyte transcript prefix */
        writeln!(f, "{}", "x".repeat(300 * 1024)).unwrap();
        writeln!(
            f,
            r#"{{"payload":{{"rate_limits":{{"primary":{{"used_percent":7.5,"resets_in_seconds":60}}}}}}}}"#
        )
        .unwrap();
        let quota = codex_quota(&root).expect("quota");
        assert_eq!(quota["windows"][0]["usedPercent"], 7.5);
        std::fs::remove_dir_all(&root).ok();
    }
}
