use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
#[allow(dead_code)] // used by the rollout test
pub fn newest_codex_rollout(home: &Path) -> Option<PathBuf> {
    newest_codex_rollout_at(&home.join(".codex").join("sessions"))
}

fn newest_codex_rollout_at(dir: &Path) -> Option<PathBuf> {
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
    walk(dir, &mut best);
    best.map(|(_, path)| path)
}

/// Codex appends a full rate-limit snapshot on each token-count event, so the
/// last one in the newest rollout is the current state.
pub fn codex_quota(home: &Path) -> Option<Value> {
    codex_quota_at(&home.join(".codex"))
}

pub fn codex_quota_at(config: &Path) -> Option<Value> {
    use std::io::{Read, Seek, SeekFrom};
    let path = newest_codex_rollout_at(&config.join("sessions"))?;
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

/// Antigravity's CLI answers `/usage` with one JSON object whose
/// `command.data.groups[].buckets[]` lists the remaining quota per window.
/// Only lines starting with `{` are JSON; the CLI may print log noise first.
pub fn parse_antigravity_output(text: &str) -> Option<Value> {
    let value = text.lines().rev().find_map(|line| {
        let line = line.trim();
        if !line.starts_with('{') {
            return None;
        }
        serde_json::from_str::<Value>(line).ok()
    })?;
    let groups = value.pointer("/command/data/groups")?.as_array()?;
    let now = unix_now();
    let mut windows = Vec::new();
    for group in groups {
        let group_name = group
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Antigravity");
        let Some(buckets) = group.get("buckets").and_then(Value::as_array) else {
            continue;
        };
        for bucket in buckets {
            let Some(remaining) = bucket.get("remaining_fraction").and_then(Value::as_f64)
            else {
                continue;
            };
            let window = bucket.get("window").and_then(Value::as_str).unwrap_or("");
            let (minutes, label) = match window {
                "weekly" => (10080, "Weekly".to_string()),
                "5h" => (300, "5h".to_string()),
                other => (
                    0,
                    bucket
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(other)
                        .to_string(),
                ),
            };
            let resets_in_seconds = bucket
                .get("reset_time")
                .and_then(Value::as_str)
                .and_then(parse_iso_seconds)
                .map(|reset| (reset - now).max(0))
                .unwrap_or(0);
            windows.push(json!({
                "kind": bucket.get("id").and_then(Value::as_str).unwrap_or(window),
                "label": format!("{group_name} \u{00b7} {label}"),
                "usedPercent": (1.0 - remaining.clamp(0.0, 1.0)) * 100.0,
                "resetsInSeconds": resets_in_seconds,
                "windowMinutes": minutes,
            }));
        }
    }
    if windows.is_empty() {
        return None;
    }
    Some(json!({ "agent": "antigravity", "plan": Value::Null, "windows": windows }))
}

/// Reset timestamps are ISO-8601 UTC (`2026-11-28T15:19:11Z`); a civil-date
/// conversion keeps the quota parser dependency-free.
fn parse_iso_seconds(text: &str) -> Option<i64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn agy_binary(home: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "agy.exe" } else { "agy" };
    let candidates = [
        home.join(".local").join("bin").join(name),
        home.join(".local").join("share").join("isle").join(name),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

/// `agy` takes a second or two and must never block the GUI thread, so the
/// quota is fetched on a worker and served from a short cache.
const AGY_TTL: Duration = Duration::from_secs(45);
static AGY_CACHE: Mutex<Option<(Instant, Option<Value>)>> = Mutex::new(None);
static AGY_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn antigravity_quota(home: &Path) -> Option<Value> {
    let cached = AGY_CACHE.lock().ok().and_then(|guard| guard.clone());
    if let Some((at, value)) = &cached {
        if at.elapsed() < AGY_TTL {
            return value.clone();
        }
    }
    if !AGY_RUNNING.swap(true, Ordering::SeqCst) {
        let home = home.to_path_buf();
        std::thread::spawn(move || {
            let value = query_antigravity(&home);
            if let Ok(mut guard) = AGY_CACHE.lock() {
                *guard = Some((Instant::now(), value));
            }
            AGY_RUNNING.store(false, Ordering::SeqCst);
        });
    }
    cached.and_then(|(_, value)| value)
}

fn query_antigravity(home: &Path) -> Option<Value> {
    let mut command = match agy_binary(home) {
        Some(path) => Command::new(path),
        None => Command::new("agy"),
    };
    command
        .args([
            "-p",
            "/usage",
            "--output-format",
            "json",
            "--print-timeout",
            "20s",
        ])
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_antigravity_output(&String::from_utf8_lossy(&output.stdout))
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

/// Claude Code has no documented quota file, but its OAuth credentials can
/// answer the same endpoint the CLI panel uses. Keep a short cache so the
/// status bar and the modal do not hammer the API.
const CLAUDE_API_TTL: Duration = Duration::from_secs(60);
static CLAUDE_API_CACHE: Mutex<Option<(Instant, String, Option<Value>)>> = Mutex::new(None);

fn claude_usage_from_api(config: &Path) -> Option<Value> {
    let key = config.to_string_lossy().to_string();
    if let Ok(guard) = CLAUDE_API_CACHE.lock() {
        if let Some((at, cached_key, value)) = guard.as_ref() {
            if cached_key == &key && at.elapsed() < CLAUDE_API_TTL {
                return value.clone();
            }
        }
    }
    let value = query_claude_usage(config);
    if let Ok(mut guard) = CLAUDE_API_CACHE.lock() {
        *guard = Some((Instant::now(), key, value.clone()));
    }
    value
}

fn query_claude_usage(config: &Path) -> Option<Value> {
    let bytes = std::fs::read(config.join(".credentials.json")).ok()?;
    let credentials: Value = serde_json::from_slice(&bytes).ok()?;
    let oauth = credentials.get("claudeAiOauth")?;
    let token = oauth.get("accessToken").and_then(Value::as_str)?.trim();
    if token.is_empty() {
        return None;
    }
    let plan = oauth
        .get("subscriptionType")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build();
    let response = agent
        .get("https://api.anthropic.com/api/oauth/usage")
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("User-Agent", "claude-code/2.1.0")
        .call()
        .ok()?;
    let body = response.into_string().ok()?;
    parse_claude_usage(&body, plan.as_deref())
}

/// The OAuth endpoint reports `{five_hour, seven_day, ...}` with either
/// `utilization` or `used_percentage`, plus an ISO `resets_at`.
fn parse_claude_usage(body: &str, plan: Option<&str>) -> Option<Value> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut windows = Vec::new();
    for (key, kind, label, minutes) in [
        ("five_hour", "session", "5h", 300_i64),
        ("seven_day", "weekly", "Weekly", 10080_i64),
        ("seven_day_opus", "weekly-opus", "Weekly (Opus)", 10080_i64),
        ("seven_day_sonnet", "weekly-sonnet", "Weekly (Sonnet)", 10080_i64),
    ] {
        let Some(entry) = value.get(key) else { continue };
        let used = entry
            .get("utilization")
            .or_else(|| entry.get("used_percentage"))
            .or_else(|| entry.get("usedPercent"))
            .and_then(Value::as_f64);
        let Some(used) = used else { continue };
        let reset = entry
            .get("resets_at")
            .and_then(|reset| match reset {
                Value::String(text) => parse_iso_seconds(text),
                Value::Number(number) => number.as_i64(),
                _ => None,
            })
            .map(|reset| (reset - unix_now()).max(0))
            .unwrap_or(0);
        windows.push(json!({
            "kind": kind,
            "label": label,
            "usedPercent": used.clamp(0.0, 100.0),
            "resetsInSeconds": reset,
            "windowMinutes": minutes,
        }));
    }
    if windows.is_empty() {
        return None;
    }
    Some(json!({ "agent": "claude", "plan": plan, "windows": windows }))
}

/// Claude Code has no documented quota file. If the user's install writes one
/// (`~/.claude/usage.json`), read the percentages; otherwise try OAuth.
pub fn claude_quota(home: &Path) -> Option<Value> {
    claude_quota_at(&home.join(".claude"))
}

pub fn claude_quota_at(config: &Path) -> Option<Value> {
    if let Some(live) = claude_usage_from_api(config) {
        return Some(live);
    }
    let bytes = std::fs::read(config.join("usage.json")).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let mut windows = Vec::new();
    for (key, entry) in value.as_object()? {
        if !entry.is_object() {
            continue;
        }
        let used = entry
            .get("usedPercent")
            .or_else(|| entry.get("used_percent"))
            .or_else(|| entry.get("utilization"))
            .or_else(|| entry.get("percentage"))
            .and_then(Value::as_f64);
        let Some(used) = used else { continue };
        let reset = entry
            .get("resetsInSeconds")
            .or_else(|| entry.get("resets_in_seconds"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        windows.push(json!({
            "kind": key,
            "usedPercent": used,
            "resetsInSeconds": reset,
            "windowMinutes": entry.get("windowMinutes").and_then(Value::as_i64).unwrap_or(0),
        }));
    }
    if windows.is_empty() {
        return None;
    }
    Some(json!({ "agent": "claude", "plan": null, "windows": windows }))
}

/// Boot-safe snapshot: local files only. `agy` is never spawned here because a
/// logged-out CLI can raise its own login window; handoff gating must not do
/// that. `snapshot_live_for` is the explicit, user-invoked variant.
pub fn snapshot_for(root: &Path, home: &Path) -> Value {
    let mut agents = Vec::new();
    if let Some(codex) = codex_quota(home) {
        agents.push(codex);
    }
    if let Some(claude) = claude_quota(home) {
        agents.push(claude);
    }
    for row in crate::agent::profile_rows(root) {
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        if row.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let base = row.get("base").and_then(Value::as_str).unwrap_or("");
        let Some(dir) = crate::agent::profile_dir_at(home, root, name) else {
            continue;
        };
        let quota = match base {
            "codex" => codex_quota_at(&dir),
            "claude" => claude_quota_at(&dir),
            _ => None,
        };
        if let Some(mut value) = quota {
            if let Value::Object(map) = &mut value {
                map.insert("agent".into(), json!(name));
            }
            agents.push(value);
        }
    }
    json!({ "ram": ram(), "agents": agents })
}

/// Used only by `omnix usage` and the Usage modal. The first read returns what
/// is cached; the worker fills it in for the next explicit read.
pub fn snapshot_live_for(root: &Path, home: &Path) -> Value {
    let mut value = snapshot_for(root, home);
    if let Some(antigravity) = antigravity_quota(home) {
        if let Value::Object(map) = &mut value {
            if let Some(agents) = map.get_mut("agents").and_then(Value::as_array_mut) {
                agents.push(antigravity);
            }
        }
    }
    value
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
    fn parses_antigravity_quota() {
        let text = r#"log noise
{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[{"name":"Gemini Models","buckets":[{"id":"gemini-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.9874580502510071,"reset_time":"2026-11-28T15:19:11Z"},{"id":"gemini-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":1,"reset_time":"2026-11-23T11:59:04Z"}]},{"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":1,"reset_time":"2026-11-30T06:59:04Z"}]}]}}}"#;
        let quota = parse_antigravity_output(text).expect("quota");
        assert_eq!(quota["agent"], "antigravity");
        let windows = quota["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0]["windowMinutes"], 10080);
        assert_eq!(windows[0]["label"], "Gemini Models \u{00b7} Weekly");
        assert!((windows[0]["usedPercent"].as_f64().unwrap() - 1.25419497489929).abs() < 1e-9);
        assert!(windows[0]["resetsInSeconds"].as_i64().unwrap() >= 0);
        assert!(parse_antigravity_output("no json").is_none());
    }

    #[test]
    fn parses_iso_reset_times() {
        assert_eq!(parse_iso_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_seconds("2026-11-28T15:19:11Z"), Some(1795879151));
        assert_eq!(parse_iso_seconds("not a date"), None);
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

    #[test]
    fn snapshot_reports_each_profile_under_its_own_name() {
        let base = std::env::temp_dir().join(format!("omnix-usage-prof-{}", std::process::id()));
        let root = base.join("repo");
        let home = base.join("home");
        std::fs::create_dir_all(root.join(".omnix")).unwrap();
        std::fs::write(
            root.join(".omnix/agents.json"),
            r#"{"codex:akun2":{},"claude:akun2":{}}"#,
        )
        .unwrap();
        let codex = home.join(".omnix/profiles/codex/akun2/sessions/2026/11/24");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(
            codex.join("rollout-a.jsonl"),
            r#"{"payload":{"rate_limits":{"primary":{"used_percent":12.0,"resets_in_seconds":30}}}}"#,
        )
        .unwrap();
        let claude = home.join(".omnix/profiles/claude/akun2");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("usage.json"),
            r#"{"five_hour":{"usedPercent":34.0,"resetsInSeconds":60}}"#,
        )
        .unwrap();
        let snapshot = snapshot_for(&root, &home);
        let rows = snapshot["agents"].as_array().unwrap();
        let names: Vec<&str> = rows
            .iter()
            .filter_map(|row| row["agent"].as_str())
            .collect();
        assert!(names.contains(&"codex:akun2"), "rows were {names:?}");
        assert!(names.contains(&"claude:akun2"), "rows were {names:?}");
        assert!(!names.contains(&"codex"), "base login must not be faked");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn parses_claude_oauth_usage() {
        let body = r#"{
          "five_hour": {"utilization": 100, "resets_at": "2026-11-24T12:00:00Z"},
          "seven_day": {"utilization": 81, "resets_at": "2026-11-25T20:00:00Z"}
        }"#;
        let quota = parse_claude_usage(body, Some("max")).expect("quota");
        assert_eq!(quota["agent"], "claude");
        assert_eq!(quota["plan"], "max");
        let windows = quota["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0]["kind"], "session");
        assert_eq!(windows[0]["usedPercent"], 100.0);
        assert_eq!(windows[1]["windowMinutes"], 10080);
        assert!(parse_claude_usage("{}", None).is_none());
    }

    #[test]
    fn reads_claude_usage_when_the_install_writes_it() {
        let root = std::env::temp_dir().join(format!("omnix-claude-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/usage.json"),
            r#"{"five_hour":{"usedPercent":42.5,"resetsInSeconds":60}}"#,
        )
        .unwrap();
        let quota = claude_quota(&root).expect("quota");
        assert_eq!(quota["agent"], "claude");
        assert_eq!(quota["windows"][0]["usedPercent"], 42.5);
        std::fs::remove_dir_all(&root).ok();
    }
}
