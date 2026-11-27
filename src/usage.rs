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
    pub label: String,
    pub used_percent: f64,
    pub resets_in_seconds: i64,
    pub window_minutes: i64,
    pub absolute_reset: bool,
}

fn codex_window_label(kind: &str, minutes: i64) -> &'static str {
    if minutes == 300 || kind == "primary" {
        "5h"
    } else if minutes == 10080 || kind == "secondary" {
        "Weekly"
    } else {
        "Quota"
    }
}

/// Pull one `payload.rate_limits` object out of a Codex rollout line.
pub fn parse_codex_line(line: &str) -> Option<(Vec<Window>, Option<String>)> {
    let value: Value = serde_json::from_str(line).ok()?;
    let limits = value.pointer("/payload/rate_limits")?;
    let now = unix_now();
    let mut windows = Vec::new();
    for kind in ["primary", "secondary"] {
        let Some(win) = limits.get(kind) else { continue };
        let Some(used) = win.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let window_minutes = win
            .get("window_minutes")
            .and_then(Value::as_i64)
            .unwrap_or(if kind == "primary" { 300 } else { 10080 });
        let (resets_in_seconds, absolute_reset) =
            if let Some(secs) = win.get("resets_in_seconds").and_then(Value::as_i64) {
                (secs, false)
            } else if let Some(at) = win.get("resets_at").and_then(Value::as_i64) {
                ((at - now).max(0), true)
            } else {
                (0, false)
            };
        windows.push(Window {
            kind: kind.to_string(),
            label: codex_window_label(kind, window_minutes).to_string(),
            used_percent: used,
            resets_in_seconds,
            window_minutes,
            absolute_reset,
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
            "windows": windows.iter().map(|w| {
                let reset = if w.absolute_reset {
                    w.resets_in_seconds.max(0)
                } else {
                    (w.resets_in_seconds - age).max(0)
                };
                json!({
                    "kind": w.kind,
                    "label": w.label,
                    "usedPercent": w.used_percent,
                    "resetsInSeconds": reset,
                    "windowMinutes": w.window_minutes,
                })
            }).collect::<Vec<_>>(),
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

/// One quota-exhaustion sighting. `confidence` is `high` for an explicit
/// "usage limit"/"quota exceeded" line, `medium` for a bare 429, and `low`
/// for weaker wording. A reset is only reported when the text carries one.
#[derive(Debug, Clone, PartialEq)]
pub struct Exhaustion {
    pub reset_at: Option<i64>,
    pub reset_local: Option<String>,
    pub reset_text: Option<String>,
    pub retryable: Option<bool>,
    pub confidence: &'static str,
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(next) = chars.next() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn parse_retryable(lower: &str) -> Option<bool> {
    let compact: String = lower.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.contains("\"retryable\":false") || compact.contains("retryable:false") {
        return Some(false);
    }
    if compact.contains("\"retryable\":true") || compact.contains("retryable:true") {
        return Some(true);
    }
    None
}

/// Detect when a headless `agy -p` run stops because of open background tasks.
pub fn detect_stalled_output(text: &str) -> Option<String> {
    let plain = strip_ansi(text);
    for line in plain.lines().rev() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("root agent idle; waiting for") {
            return Some(line.trim().chars().take(300).collect());
        }
    }
    None
}

/// Scan the tail of a pane. The caller passes the last few KiB, so this stays
/// cheap and cannot match a quota phrase from an old turn.
pub fn detect_exhaustion(text: &str) -> Option<Exhaustion> {
    let plain = strip_ansi(text);
    let lower = plain.to_ascii_lowercase();
    let strong = [
        "usage limit reached",
        "usage limit has been reached",
        "hit your usage limit",
        "you've hit your usage limit",
        "reached your usage limit",
        "you have exceeded your quota",
        "exceeded your quota",
        "quota exceeded",
        "quota exhausted",
        "out of credits",
        "ai credits balance is too low",
        "credit balance is too low",
        "insufficient_quota",
        "resource_exhausted",
        "resource exhausted",
        "weekly limit reached",
        "5-hour limit reached",
        "hour limit reached",
        "rate_limit_error",
        "overloaded_error",
    ];
    let medium = [
        "rate limit reached",
        "rate limit exceeded",
        "rate limit",
        "too many requests",
        "429 too many",
        "http 429",
        "code 429",
        "error 429",
        "status 429",
        "agy_error",
    ];
    let retryable = parse_retryable(&lower);
    let confidence = if strong.iter().any(|needle| lower.contains(needle)) {
        "high"
    } else if medium.iter().any(|needle| lower.contains(needle))
        || (lower.contains("429") && (lower.contains("quota") || lower.contains("rate") || lower.contains("agy_error")))
    {
        "medium"
    } else if lower.contains("quota") && lower.contains("limit") {
        "low"
    } else {
        return None;
    };
    let (reset_at, reset_text) = parse_reset(&plain)
        .map(|(at, text)| (Some(at), Some(text)))
        .unwrap_or((None, None));
    let reset_local = reset_at
        .filter(|&at| at > 0)
        .map(|at| crate::control::local_time(at as u64));
    Some(Exhaustion {
        reset_at,
        reset_local,
        reset_text,
        retryable,
        confidence,
    })
}

fn find_pipe_epoch(text: &str) -> Option<(i64, String)> {
    let mut from = 0;
    while let Some(pos) = text[from..].find('|') {
        let rest = text[from + pos + 1..].trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if (9..=12).contains(&digits.len()) {
            if let Ok(val) = digits.parse::<i64>() {
                let secs = if digits.len() > 10 { val / 1000 } else { val };
                if secs > 1_700_000_000 {
                    return Some((secs, format!("epoch {secs}")));
                }
            }
        }
        from += pos + 1;
    }
    None
}

fn reset_marker(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    [
        "resets at",
        "reset at",
        "resets on",
        "reset on",
        "try again at",
        "try again in",
        "retry after",
        "available at",
        "available in",
    ]
    .iter()
    .filter_map(|needle| lower.find(needle))
    .min()
}

/// Parse the reset nearest the marker. Supports ISO-8601, `3:14 PM`,
/// `14:05`, and `in 2 hours`/`retry after 90 seconds`.
fn parse_reset(text: &str) -> Option<(i64, String)> {
    let now = unix_now();
    if let Some((epoch, label)) = find_pipe_epoch(text) {
        return Some((epoch, label));
    }
    let start = reset_marker(text).unwrap_or(0);
    let tail = &text[start..];
    let lower = tail.to_ascii_lowercase();

    if let Some(at) = find_iso(tail) {
        let seconds = parse_iso_seconds(&at)?;
        if seconds > now {
            return Some((seconds, at));
        }
    }

    for unit in ["second", "minute", "hour", "day"] {
        for needle in [format!("in "), format!("after ")] {
            let pattern = format!("{needle}");
            let mut from = 0;
            while let Some(pos) = lower[from..].find(&pattern) {
                let idx = from + pos + pattern.len();
                let rest = lower[idx..].trim_start();
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(value) = digits.parse::<i64>() {
                    let after = rest[digits.len()..].trim_start();
                    if after.starts_with(unit) {
                        let scale = match unit {
                            "second" => 1,
                            "minute" => 60,
                            "hour" => 3600,
                            _ => 86_400,
                        };
                        let when = now + value * scale;
                        return Some((when, tail[..tail.len().min(80)].trim().to_string()));
                    }
                }
                from = idx;
            }
        }
    }

    let clock = find_clock(tail)?;
    let at = next_local_clock(clock.0, clock.1)?;
    Some((at, tail[..tail.len().min(80)].trim().to_string()))
}

fn find_iso(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for start in 0..bytes.len().saturating_sub(19) {
        let Some(slice) = text.get(start..start + 19) else {
            continue;
        };
        if slice.as_bytes().get(4) == Some(&b'-')
            && slice.as_bytes().get(10) == Some(&b'T')
            && slice.as_bytes().get(13) == Some(&b':')
            && slice[..4].chars().all(|c| c.is_ascii_digit())
        {
            return Some(slice.to_string());
        }
    }
    None
}

/// `(hour, minute)` from the first clock-like token.
fn find_clock(text: &str) -> Option<(u32, u32)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b':' && i + 3 <= bytes.len()
                && bytes[i + 1].is_ascii_digit()
                && bytes[i + 2].is_ascii_digit()
            {
                let hour: u32 = text[start..i].parse().ok()?;
                let minute: u32 = text[i + 1..i + 3].parse().ok()?;
                if hour <= 23 && minute <= 59 {
                    let mut hour = hour;
                    let rest = text[i + 3..].trim_start();
                    let lower = rest.to_ascii_lowercase();
                    if lower.starts_with("pm") && hour < 12 {
                        hour += 12;
                    } else if lower.starts_with("am") && hour == 12 {
                        hour = 0;
                    }
                    return Some((hour, minute));
                }
            }
        }
        i += 1;
    }
    None
}

/// Next occurrence of a local wall-clock time. The caller gets an epoch second
/// that is correct across a DST boundary because it works from the current
/// local second-of-day rather than assuming a fixed offset.
fn next_local_clock(hour: u32, minute: u32) -> Option<i64> {
    let now = unix_now();
    #[cfg(unix)]
    {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let stamp = now as libc::time_t;
        if unsafe { libc::localtime_r(&stamp, &mut tm) }.is_null() {
            return None;
        }
        let target = hour as i64 * 3600 + minute as i64 * 60;
        let current = tm.tm_hour as i64 * 3600 + tm.tm_min as i64 * 60 + tm.tm_sec as i64;
        let mut delta = target - current;
        if delta <= 0 {
            delta += 86_400;
        }
        return Some(now + delta);
    }
    #[cfg(not(unix))]
    {
        let _ = (hour, minute);
        None
    }
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

pub fn antigravity_quota_sync(home: &Path) -> Option<Value> {
    if let Ok(guard) = AGY_CACHE.lock() {
        if let Some((at, value)) = guard.as_ref() {
            if at.elapsed() < AGY_TTL && value.is_some() {
                return value.clone();
            }
        }
    }
    let value = query_antigravity(home);
    if let Ok(mut guard) = AGY_CACHE.lock() {
        *guard = Some((Instant::now(), value.clone()));
    }
    value
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

fn exhaustion_events(home: &Path) -> Vec<(String, Value)> {
    let path = home.join(".omnix").join("events.jsonl");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let now = unix_now();
    let mut latest: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for line in text.lines().rev().take(500) {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = row.get("kind").and_then(Value::as_str);
        if kind != Some("agent-quota-exhausted") && kind != Some("quota-exhausted") {
            continue;
        }
        let Some(agent) = row.get("agent").and_then(Value::as_str) else {
            continue;
        };
        let at = row.get("at").and_then(Value::as_i64).unwrap_or(now);
        let reset = row.get("resetAt").and_then(Value::as_i64);
        // Output-derived exhaustion is a short-lived hint: without a reset time
        // it only holds for ten minutes.
        let stale = reset.map(|at| at <= now).unwrap_or_else(|| at + 600 <= now);
        if stale {
            continue;
        }
        let key = if let Some(model) = row
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            if agent == "antigravity" || agent.starts_with("antigravity:") {
                format!("{agent}|{}", antigravity_group_key(model))
            } else {
                agent.to_string()
            }
        } else {
            agent.to_string()
        };
        latest.entry(key).or_insert(row);
    }
    latest.into_iter().collect()
}

fn decorate_agents(agents: &mut Vec<Value>, home: &Path) {
    let now = unix_now();
    let events = exhaustion_events(home);
    for entry in agents.iter_mut() {
        let Some(map) = entry.as_object_mut() else {
            continue;
        };
        map.insert("state".into(), json!("ok"));
        map.insert("source".into(), json!("local"));
        map.insert("confidence".into(), json!("high"));
        map.insert("exhausted".into(), json!(false));
        if let Some(windows) = map.get_mut("windows").and_then(Value::as_array_mut) {
            for window in windows {
                if let Some(window) = window.as_object_mut() {
                    window.insert("exhausted".into(), json!(false));
                    let reset = window
                        .get("resetsInSeconds")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    if reset > 0 {
                        window.insert("resetAt".into(), json!(now + reset));
                    }
                }
            }
        }
    }
    for (key, event) in events {
        let agent = key.split_once('|').map(|(a, _)| a).unwrap_or(&key);
        let group = key.split_once('|').map(|(_, g)| g);
        let idx = agents
            .iter()
            .position(|entry| entry.get("agent").and_then(Value::as_str) == Some(agent));
        let idx = match idx {
            Some(idx) => idx,
            None => {
                agents.push(json!({
                    "agent": agent,
                    "plan": null,
                    "windows": [],
                }));
                agents.len() - 1
            }
        };
        let entry = &mut agents[idx];
        if let Some(map) = entry.as_object_mut() {
            if let Some(group) = group {
                let mut groups = map
                    .get("exhaustedGroups")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !groups.iter().any(|v| v.as_str() == Some(group)) {
                    groups.push(json!(group));
                }
                map.insert("exhaustedGroups".into(), Value::Array(groups));
                if let Some(windows) = map.get_mut("windows").and_then(Value::as_array_mut) {
                    for w in windows.iter_mut() {
                        let label = w.get("label").and_then(Value::as_str).unwrap_or("");
                        if label_matches_group(label, group) {
                            if let Some(wm) = w.as_object_mut() {
                                wm.insert("exhausted".into(), json!(true));
                            }
                        }
                    }
                }
            }
            let windows_empty = map
                .get("windows")
                .and_then(Value::as_array)
                .map(|w| w.is_empty())
                .unwrap_or(true);
            // Official window data wins: only fall back to the output text when
            // no window is known for this agent.
            if group.is_none() && windows_empty {
                map.insert("state".into(), json!("exhausted"));
                map.insert("exhausted".into(), json!(true));
            }
            map.insert("source".into(), json!("output"));
            if let Some(confidence) = event.get("confidence").and_then(Value::as_str) {
                map.insert("confidence".into(), json!(confidence));
            }
            if let Some(reset) = event.get("resetAt").and_then(Value::as_i64) {
                map.insert("resetAt".into(), json!(reset));
            }
            if let Some(local) = event.get("resetLocal").and_then(Value::as_str) {
                map.insert("resetLocal".into(), json!(local));
            }
            if let Some(model) = event.get("model").and_then(Value::as_str) {
                map.insert("model".into(), json!(model));
            }
        }
    }
}

fn unknown_agents(agents: &[Value]) -> Vec<String> {
    ["codex", "claude", "antigravity"]
        .into_iter()
        .filter(|name| {
            !agents.iter().any(|entry| {
                entry
                    .get("agent")
                    .and_then(Value::as_str)
                    .map(|agent| agent == *name || agent.starts_with(&format!("{name}:")))
                    .unwrap_or(false)
            })
        })
        .map(|name| name.to_string())
        .collect()
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
    decorate_agents(&mut agents, home);
    let unknown = unknown_agents(&agents);
    json!({ "ram": ram(), "agents": agents, "unknown": unknown })
}

/// Used only by `omnix usage` and the Usage modal. The first read returns what
/// is cached; the worker fills it in for the next explicit read.
pub fn snapshot_live_for(root: &Path, home: &Path) -> Value {
    let mut value = snapshot_for(root, home);
    if let Some(antigravity) = antigravity_quota(home) {
        merge_antigravity(&mut value, home, antigravity);
    }
    value
}

/// Synchronous snapshot for the CLI `omnix usage` and `--check`. Runs `agy`
/// directly instead of returning a cold cache miss on the first call.
pub fn snapshot_live_sync_for(root: &Path, home: &Path) -> Value {
    let mut value = snapshot_for(root, home);
    if let Some(antigravity) = antigravity_quota_sync(home) {
        merge_antigravity(&mut value, home, antigravity);
    }
    value
}

fn merge_antigravity(value: &mut Value, home: &Path, antigravity: Value) {
    if let Value::Object(map) = value {
        let mut unknown = None;
        if let Some(agents) = map.get_mut("agents").and_then(Value::as_array_mut) {
            agents.retain(|a| a.get("agent").and_then(Value::as_str) != Some("antigravity"));
            agents.push(antigravity);
            decorate_agents(agents, home);
            unknown = Some(json!(unknown_agents(agents)));
        }
        if let Some(unknown) = unknown {
            map.insert("unknown".into(), unknown);
        }
    }
}

pub fn antigravity_group_key(model: &str) -> &'static str {
    let lower = model.trim().to_ascii_lowercase();
    if lower.contains("gemini") {
        "gemini"
    } else if lower.contains("claude")
        || lower.contains("sonnet")
        || lower.contains("opus")
        || lower.contains("haiku")
        || lower.contains("gpt")
    {
        "claude-gpt"
    } else {
        "all"
    }
}

fn label_matches_group(label: &str, group: &str) -> bool {
    let lower = label.to_ascii_lowercase();
    match group {
        "gemini" => lower.contains("gemini"),
        "claude-gpt" => lower.contains("claude") || lower.contains("gpt") || lower.contains("3p"),
        _ => true,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaCheck {
    pub agent: String,
    pub model: Option<String>,
    pub exhausted: bool,
    pub known: bool,
    pub reason: Option<String>,
    pub reset_at: Option<i64>,
    pub reset_local: Option<String>,
}

/// Split `<agent>[:<model>]` without clobbering `codex:akun2` profile names
/// registered in `.omnix/agents.json`.
pub fn split_agent_model(root: &Path, spec: &str) -> (String, Option<String>) {
    let spec = spec.trim();
    if let Some((left, right)) = spec.split_once(':') {
        let settings = crate::agent::load_settings(root);
        if settings.contains_key(spec) {
            return (spec.to_string(), None);
        }
        return (left.trim().to_string(), Some(right.trim().to_string()));
    }
    (spec.to_string(), None)
}

pub fn check_quota_in_snapshot(snapshot: &Value, agent: &str, model: Option<&str>) -> QuotaCheck {
    let entry = snapshot
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|row| row.get("agent").and_then(Value::as_str) == Some(agent))
                .or_else(|| {
                    let base = crate::agent::base_agent(agent);
                    arr.iter()
                        .find(|row| row.get("agent").and_then(Value::as_str) == Some(base))
                })
        });
    let Some(entry) = entry else {
        return QuotaCheck {
            agent: agent.to_string(),
            model: model.map(str::to_string),
            exhausted: false,
            known: false,
            reason: None,
            reset_at: None,
            reset_local: None,
        };
    };

    let group = if crate::agent::base_agent(agent) == "antigravity" {
        model.map(antigravity_group_key)
    } else {
        None
    };

    let matched_windows: Vec<&Value> = entry
        .get("windows")
        .and_then(Value::as_array)
        .map(|ws| {
            ws.iter()
                .filter(|w| {
                    group
                        .map(|g| {
                            label_matches_group(
                                w.get("label").and_then(Value::as_str).unwrap_or(""),
                                g,
                            )
                        })
                        .unwrap_or(true)
                })
                .collect()
        })
        .unwrap_or_default();

    let window_exhausted = matched_windows.iter().find(|w| {
        w.get("exhausted").and_then(Value::as_bool) == Some(true)
            || w.get("usedPercent").and_then(Value::as_f64).unwrap_or(0.0) >= 99.5
    });

    if let Some(w) = window_exhausted {
        let label = w
            .get("label")
            .or_else(|| w.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("quota");
        let used = w.get("usedPercent").and_then(Value::as_f64).unwrap_or(100.0);
        let reset_at = w
            .get("resetAt")
            .and_then(Value::as_i64)
            .or_else(|| entry.get("resetAt").and_then(Value::as_i64));
        let reset_local = reset_at
            .filter(|&at| at > 0)
            .map(|at| crate::control::local_time(at as u64));
        return QuotaCheck {
            agent: agent.to_string(),
            model: model.map(str::to_string),
            exhausted: true,
            known: true,
            reason: Some(format!("{agent} ({label}) {used:.0}% used")),
            reset_at,
            reset_local,
        };
    }

    let group_exhausted = group
        .and_then(|g| {
            entry
                .get("exhaustedGroups")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().any(|v| v.as_str() == Some(g)))
        })
        .unwrap_or(false);

    if entry.get("state").and_then(Value::as_str) == Some("exhausted") || group_exhausted {
        let reset_at = entry.get("resetAt").and_then(Value::as_i64);
        let reset_local = entry
            .get("resetLocal")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                reset_at
                    .filter(|&at| at > 0)
                    .map(|at| crate::control::local_time(at as u64))
            });
        return QuotaCheck {
            agent: agent.to_string(),
            model: model.map(str::to_string),
            exhausted: true,
            known: true,
            reason: Some(format!("{agent} quota exhausted")),
            reset_at,
            reset_local,
        };
    }

    let _ = matched_windows.len();
    QuotaCheck {
        agent: agent.to_string(),
        model: model.map(str::to_string),
        exhausted: false,
        known: true,
        reason: None,
        reset_at: None,
        reset_local: None,
    }
}

pub fn check_quota(root: &Path, home: &Path, agent: &str, model: Option<&str>, live: bool) -> QuotaCheck {
    let snapshot = if live {
        snapshot_live_sync_for(root, home)
    } else {
        snapshot_for(root, home)
    };
    check_quota_in_snapshot(&snapshot, agent, model)
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

    #[test]
    fn detects_quota_exhaustion_and_relative_reset() {
        let plain = detect_exhaustion("You've hit your usage limit. Retry after 90 seconds.")
            .expect("limit");
        assert_eq!(plain.confidence, "high");
        let delta = plain.reset_at.expect("reset") - unix_now();
        assert!((85..=95).contains(&delta), "delta was {delta}");

        let clock = detect_exhaustion("quota exceeded; try again at 3:14 PM").expect("clock");
        assert!(clock.reset_at.expect("reset") > unix_now());

        let iso = detect_exhaustion("usage limit reached, resets at 2099-01-01T00:00:00Z")
            .expect("iso");
        assert_eq!(iso.reset_at, Some(4_070_908_800));
    }

    #[test]
    fn detects_a_bare_429_with_medium_confidence() {
        let hit = detect_exhaustion("\u{1b}[31m429 Too Many Requests\u{1b}[0m").expect("429");
        assert_eq!(hit.confidence, "medium");
        assert!(detect_exhaustion("cargo test passed").is_none());
    }

    #[test]
    fn quota_events_mark_an_agent_exhausted_without_local_files() {
        let base = std::env::temp_dir().join(format!("omnix-usage-event-{}", std::process::id()));
        let root = base.join("repo");
        let home = base.join("home");
        std::fs::create_dir_all(home.join(".omnix")).unwrap();
        let reset = unix_now() + 3600;
        std::fs::write(
            home.join(".omnix/events.jsonl"),
            format!(
                "{{\"kind\":\"quota-exhausted\",\"agent\":\"codex\",\"at\":{},\"resetAt\":{},\"confidence\":\"high\"}}\n",
                unix_now(),
                reset
            ),
        )
        .unwrap();
        let snapshot = snapshot_for(&root, &home);
        let entry = snapshot["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["agent"] == "codex")
            .expect("codex row");
        assert_eq!(entry["state"], "exhausted");
        assert_eq!(entry["source"], "output");
        assert_eq!(entry["resetAt"], reset);
        assert_eq!(entry["windows"].as_array().unwrap().len(), 0);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn no_quota_data_is_unknown_not_zero_percent() {
        let base = std::env::temp_dir().join(format!("omnix-usage-unknown-{}", std::process::id()));
        let root = base.join("repo");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let snapshot = snapshot_for(&root, &home);
        assert!(snapshot["agents"].as_array().unwrap().is_empty());
        let unknown = snapshot["unknown"].as_array().unwrap();
        assert!(unknown.iter().any(|name| name == "codex"));
        assert!(unknown.iter().any(|name| name == "claude"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn parses_codex_resets_at_timestamps() {
        let future = unix_now() + 1800;
        let line = format!(
            r#"{{"payload":{{"rate_limits":{{"primary":{{"used_percent":34.0,"window_minutes":300,"resets_at":{future}}},"secondary":{{"used_percent":78.0,"window_minutes":10080,"resets_at":{}}},"plan_type":"plus"}}}}}}"#,
            future + 86400
        );
        let (windows, plan) = parse_codex_line(&line).expect("parsed");
        assert_eq!(plan.as_deref(), Some("plus"));
        assert_eq!(windows[0].label, "5h");
        assert!(windows[0].resets_in_seconds >= 1790);
        assert!(windows[0].absolute_reset);
        assert_eq!(windows[1].label, "Weekly");
        assert!(windows[1].resets_in_seconds >= 88000);
    }

    #[test]
    fn detects_provider_specific_exhaustion_patterns() {
        let agy = detect_exhaustion(
            r#"error: Your AI credits balance is too low to continue. AGY_ERROR: {"short_error":"RESOURCE_EXHAUSTED (code 429) ...","retryable":false}"#,
        )
        .expect("antigravity");
        assert_eq!(agy.confidence, "high");
        assert_eq!(agy.retryable, Some(false));

        let future = unix_now() + 7200;
        let claude = detect_exhaustion(&format!("Claude AI usage limit reached|{future}"))
            .expect("claude");
        assert_eq!(claude.confidence, "high");
        assert_eq!(claude.reset_at, Some(future));
        assert!(claude.reset_local.is_some());

        let codex = detect_exhaustion("You've hit your usage limit. Try again at 6:47 PM.")
            .expect("codex");
        assert_eq!(codex.confidence, "high");
        assert!(codex.reset_at.is_some());

        let stalled = detect_stalled_output("root agent idle; waiting for 2 background task(s)");
        assert!(stalled.is_some());
    }

    #[test]
    fn check_quota_distinguishes_antigravity_model_groups() {
        let snap = json!({
            "agents": [
                {
                    "agent": "antigravity",
                    "state": "ok",
                    "windows": [
                        { "kind": "gemini-5h", "label": "Gemini Models · 5h", "usedPercent": 4.0, "resetsInSeconds": 3600 },
                        { "kind": "3p-5h", "label": "Claude and GPT models · 5h", "usedPercent": 100.0, "resetsInSeconds": 3600 }
                    ]
                }
            ]
        });
        let gem = check_quota_in_snapshot(&snap, "antigravity", Some("gemini-4-neon-high"));
        assert!(!gem.exhausted);
        assert!(gem.known);

        let claude = check_quota_in_snapshot(&snap, "antigravity", Some("claude-sonnet-5-5-high"));
        assert!(claude.exhausted);
        assert!(claude.known);
    }

    #[test]
    fn official_windows_beat_output_quota_hint() {
        let root = std::env::temp_dir().join(format!("omnix-quota-win-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::create_dir_all(root.join(".omnix")).unwrap();
        std::fs::write(
            root.join(".claude/usage.json"),
            r#"{"five_hour":{"usedPercent":8,"resetsInSeconds":3600,"windowMinutes":300},"seven_day":{"usedPercent":6,"resetsInSeconds":3600,"windowMinutes":10080}}"#,
        )
        .unwrap();
        let hint = serde_json::json!({
            "kind": "agent-quota-exhausted",
            "agent": "claude",
            "at": unix_now() - 30,
            "model": null,
            "resetAt": null,
            "resetLocal": "16:40 WIB",
            "confidence": "high"
        });
        std::fs::write(root.join(".omnix/events.jsonl"), format!("{hint}\n")).unwrap();
        let snap = snapshot_for(&root, &root);
        let check = check_quota_in_snapshot(&snap, "claude", None);
        assert!(!check.exhausted, "official windows win over output text");
        assert!(check.known);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn output_quota_hint_expires_and_needs_missing_windows() {
        let root = std::env::temp_dir().join(format!("omnix-quota-hint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".omnix")).unwrap();
        let write = |at: i64| {
            let hint = serde_json::json!({
                "kind": "agent-quota-exhausted",
                "agent": "claude",
                "at": at,
                "model": null,
                "resetAt": null,
                "confidence": "high"
            });
            std::fs::write(root.join(".omnix/events.jsonl"), format!("{hint}\n")).unwrap();
        };
        write(unix_now() - 30);
        let fresh = snapshot_for(&root, &root);
        assert!(check_quota_in_snapshot(&fresh, "claude", None).exhausted);
        write(unix_now() - 1200);
        let stale = snapshot_for(&root, &root);
        assert!(!check_quota_in_snapshot(&stale, "claude", None).exhausted);
        let _ = std::fs::remove_dir_all(&root);
    }
}
