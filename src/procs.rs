//! Process-tree memory accounting for the app and every terminal it owns.
//!
//! Linux reads `/proc` directly. macOS asks `ps` for a flat table and Windows
//! asks CIM once per snapshot. The UI stops polling while the window is
//! hidden, so the fallbacks never run as an invisible background loop.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub rss: u64,
    pub name: String,
}

/// One flat process list plus its parent index, so every pane can be summed
/// against the same snapshot without rebuilding the tree.
pub struct Table {
    procs: Vec<Proc>,
    index: HashMap<u32, usize>,
    children: HashMap<u32, Vec<usize>>,
}

impl Table {
    pub fn new(procs: Vec<Proc>) -> Self {
        let mut index = HashMap::with_capacity(procs.len());
        let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
        for (at, proc) in procs.iter().enumerate() {
            index.insert(proc.pid, at);
            children.entry(proc.ppid).or_default().push(at);
        }
        Self {
            procs,
            index,
            children,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.procs.is_empty()
    }

    /// Busiest descendant names under `root`, heaviest first. The pane row
    /// uses these as a compact "what is actually running" label.
    pub fn top_names(&self, root: u32, limit: usize) -> Vec<String> {
        if root == 0 || limit == 0 {
            return Vec::new();
        }
        let mut stack = vec![root];
        let mut seen: HashSet<u32> = HashSet::new();
        let mut found: Vec<(u64, String)> = Vec::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(kids) = self.children.get(&pid) {
                for &index in kids {
                    let child = &self.procs[index];
                    if !child.name.is_empty() {
                        found.push((child.rss, child.name.clone()));
                    }
                    stack.push(child.pid);
                }
            }
        }
        found.sort_by_key(|row| std::cmp::Reverse(row.0));
        let mut names = Vec::new();
        for (_, name) in found {
            let leaf = name.rsplit('/').next().unwrap_or(&name).to_string();
            if !names.contains(&leaf) {
                names.push(leaf);
                if names.len() >= limit {
                    break;
                }
            }
        }
        names
    }

    /// Resident bytes and process count for `root` plus all descendants.
    pub fn sum_tree(&self, root: u32) -> (u64, usize) {
        if root == 0 {
            return (0, 0);
        }
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        let mut rss = 0u64;
        let mut count = 0usize;
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(&index) = self.index.get(&pid) {
                rss = rss.saturating_add(self.procs[index].rss);
                count += 1;
            }
            if let Some(kids) = self.children.get(&pid) {
                for &index in kids {
                    stack.push(self.procs[index].pid);
                }
            }
        }
        (rss, count)
    }

    /// Processes under `root`, heaviest first. Used by the diagnostics command
    /// and by the UI to answer "what is actually holding the RAM".
    pub fn descendants(&self, root: u32) -> Vec<&Proc> {
        if root == 0 {
            return Vec::new();
        }
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(&index) = self.index.get(&pid) {
                found.push(&self.procs[index]);
            }
            if let Some(kids) = self.children.get(&pid) {
                for &index in kids {
                    stack.push(self.procs[index].pid);
                }
            }
        }
        found.sort_by_key(|row| std::cmp::Reverse(row.rss));
        found
    }

    /// Nearest ancestor of `pid` whose executable is `name`. A CLI started
    /// inside an Omnix terminal uses this to report the whole app tree instead
    /// of only itself. Paths and a Windows `.exe` suffix are ignored.
    pub fn ancestor_named(&self, pid: u32, name: &str) -> Option<u32> {
        let mut cursor = self.procs.get(*self.index.get(&pid)?)?.ppid;
        let mut seen = HashSet::new();
        while cursor != 0 && seen.insert(cursor) {
            let proc = &self.procs[*self.index.get(&cursor)?];
            let leaf = proc.name.rsplit('/').next().unwrap_or(&proc.name);
            let leaf = leaf.strip_suffix(".exe").unwrap_or(leaf);
            if leaf == name {
                return Some(cursor);
            }
            cursor = proc.ppid;
        }
        None
    }
}

pub fn snapshot() -> Table {
    Table::new(imp::snapshot())
}

/// Launch-time identity from `/proc/<pid>`: start ticks plus the full command
/// line. `/proc/<pid>/cmdline` can read back empty for a moment right after
/// `exec`, so it is retried before giving up. `None` when the process is gone
/// or the platform has no `/proc`.
#[cfg(target_os = "linux")]
pub fn identity(pid: u32) -> Option<(u64, String)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    let started: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    for _ in 0..20 {
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            return None;
        };
        let cmdline = raw
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !cmdline.is_empty() {
            return Some((started, cmdline));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn identity(_pid: u32) -> Option<(u64, String)> {
    None
}

#[cfg(target_os = "linux")]
fn state_of(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().next()?.chars().next()
}

/// Liveness, with a zombie on Linux already counted as gone.
pub fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        !matches!(state_of(pid), None | Some('Z'))
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// True only while `pid` still carries the identity captured at launch. On a
/// platform without `/proc` a record with no identity keeps the old PID
/// semantics; a record that has one can never be proven and is refused.
#[cfg(target_os = "linux")]
pub fn identity_matches(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    match (identity(pid), started, exe) {
        (Some((actual_started, actual_exe)), Some(started), Some(exe)) => {
            actual_started == started && actual_exe == exe
        }
        _ => false,
    }
}

#[cfg(not(target_os = "linux"))]
pub fn identity_matches(_pid: u32, started: Option<u64>, _exe: Option<&str>) -> bool {
    started.is_none()
}

/// Alive *and* still the recorded process.
pub fn alive_and_ours(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    identity_matches(pid, started, exe) && alive(pid)
}

#[cfg(unix)]
fn wait_gone(pid: u32, ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        if !alive(pid) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// TERM, wait, then KILL — but only when the process still matches its launch
/// identity. Returns true when it is gone.
#[cfg(unix)]
pub fn stop(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    if pid == 0 || pid == std::process::id() || !identity_matches(pid, started, exe) {
        return false;
    }
    // SAFETY: identity was just proven, so this signals the recorded process.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if wait_gone(pid, 3000) {
        return true;
    }
    if !identity_matches(pid, started, exe) {
        // The PID was recycled while we waited; never KILL the newcomer.
        return true;
    }
    // SAFETY: identity was re-proven after the wait.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    wait_gone(pid, 1000)
}

#[cfg(windows)]
pub fn stop(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    if pid == 0 || !identity_matches(pid, started, exe) {
        return false;
    }
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
pub fn stop(_pid: u32, _started: Option<u64>, _exe: Option<&str>) -> bool {
    false
}

/// Same as [`stop`] for a process group led by `pid`. Used by previews, which
/// start the server in its own group so npm/node children leave with it.
#[cfg(unix)]
pub fn stop_group(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    if pid == 0 || !identity_matches(pid, started, exe) {
        return false;
    }
    let group = -(pid as i32);
    // SAFETY: the group leader's identity was just proven.
    unsafe { libc::kill(group, libc::SIGTERM) };
    if wait_gone(pid, 3000) {
        return true;
    }
    if !identity_matches(pid, started, exe) {
        return true;
    }
    // SAFETY: the leader still matches, so the group is still ours.
    unsafe { libc::kill(group, libc::SIGKILL) };
    wait_gone(pid, 1000)
}

#[cfg(not(unix))]
pub fn stop_group(pid: u32, started: Option<u64>, exe: Option<&str>) -> bool {
    stop(pid, started, exe)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Proc;
    use std::fs;

    pub fn snapshot() -> Vec<Proc> {
        let Ok(entries) = fs::read_dir("/proc") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else {
                continue;
            };
            let mut ppid = 0;
            let mut rss = 0;
            let mut comm = String::new();
            for line in status.lines() {
                if let Some(value) = line.strip_prefix("Name:") {
                    comm = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("PPid:") {
                    ppid = value.trim().parse().unwrap_or(0);
                } else if let Some(value) = line.strip_prefix("VmRSS:") {
                    rss = value
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u64>().ok())
                        .unwrap_or(0)
                        * 1024;
                }
            }
            if let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) {
                if let Some(first) = cmdline.split(|b| *b == 0).next() {
                    let path = String::from_utf8_lossy(first);
                    if let Some(leaf) = path.rsplit('/').next().filter(|s| !s.is_empty()) {
                        comm = leaf.to_string();
                    }
                }
            }
            out.push(Proc {
                pid,
                ppid,
                rss,
                name: comm,
            });
        }
        out
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Proc;
    use std::process::Command;

    pub fn snapshot() -> Vec<Proc> {
        let Ok(out) = Command::new("ps")
            .args(["-axo", "pid=,ppid=,rss=,comm="])
            .output()
        else {
            return Vec::new();
        };
        parse_ps(&String::from_utf8_lossy(&out.stdout))
    }

    fn parse_ps(text: &str) -> Vec<Proc> {
        let mut out = Vec::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(pid), Some(ppid), Some(rss)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(pid) = pid.parse() else { continue };
            let Ok(ppid) = ppid.parse() else { continue };
            let Ok(rss) = rss.parse::<u64>() else {
                continue;
            };
            out.push(Proc {
                pid,
                ppid,
                rss: rss * 1024,
                name: parts.collect::<Vec<_>>().join(" "),
            });
        }
        out
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::Proc;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn snapshot() -> Vec<Proc> {
        let script = "Get-CimInstance Win32_Process | ForEach-Object { \
                      \"$($_.ProcessId)|$($_.ParentProcessId)|$($_.WorkingSetSize)|$($_.Name)\" }";
        let Ok(out) = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return Vec::new();
        };
        let mut procs = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.trim().split('|');
            let (Some(pid), Some(ppid), Some(rss)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(pid), Ok(ppid), Ok(rss)) =
                (pid.parse::<u32>(), ppid.parse::<u32>(), rss.parse::<u64>())
            else {
                continue;
            };
            procs.push(Proc {
                pid,
                ppid,
                rss,
                name: parts.next().unwrap_or("").to_string(),
            });
        }
        procs
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod imp {
    use super::Proc;

    pub fn snapshot() -> Vec<Proc> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, rss: u64, name: &str) -> Proc {
        Proc {
            pid,
            ppid,
            rss,
            name: name.into(),
        }
    }

    #[test]
    fn counts_every_child_and_names_the_heaviest() {
        let table = Table::new(vec![
            proc(1, 0, 10, "sh"),
            proc(2, 1, 30, "node"),
            proc(3, 2, 5, "worker"),
            proc(4, 1, 20, "codex"),
        ]);
        assert_eq!(table.sum_tree(1), (65, 4));
        assert_eq!(table.sum_tree(2), (35, 2));
        assert_eq!(table.top_names(1, 2), vec!["node", "codex"]);
        assert_eq!(table.top_names(99, 3), Vec::<String>::new());
        let names: Vec<&str> = table
            .descendants(1)
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["node", "codex", "sh", "worker"]);
        assert!(table.descendants(99).is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_refuses_a_pid_that_no_longer_matches_its_identity() {
        let mut child = std::process::Command::new("sleep")
            .args(["60"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let (started, _exe) = identity(pid).unwrap();
        // A recycled PID: same number, different start time and command line.
        assert!(!stop(pid, Some(started + 1), Some("not-the-worker")));
        assert!(alive(pid), "a mismatched PID must never be signalled");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_ends_the_process_whose_identity_matches() {
        let mut child = std::process::Command::new("sleep")
            .args(["60"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let (started, exe) = identity(pid).unwrap();
        assert!(stop(pid, Some(started), Some(&exe)));
        assert!(!alive(pid));
        let _ = child.wait();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_recycled_pid_is_not_alive_and_ours() {
        let mut child = std::process::Command::new("sleep")
            .args(["60"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let (started, exe) = identity(pid).unwrap();
        assert!(alive_and_ours(pid, Some(started), Some(&exe)));
        assert!(!alive_and_ours(pid, Some(started + 1), Some(&exe)));
        assert!(!alive_and_ours(pid, Some(started), Some("other")));
        assert!(!alive_and_ours(pid, None, None));
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn finds_the_nearest_ancestor_by_executable_name() {
        let table = Table::new(vec![
            proc(1, 0, 10, "omnix"),
            proc(2, 1, 20, "bash"),
            proc(3, 2, 5, "omnix"),
            proc(4, 0, 7, "/Applications/Omnix.app/Contents/MacOS/omnix"),
            proc(5, 0, 8, "omnix.exe"),
            proc(6, 4, 3, "bash"),
            proc(7, 5, 3, "bash"),
        ]);
        assert_eq!(table.ancestor_named(3, "omnix"), Some(1));
        assert_eq!(table.ancestor_named(6, "omnix"), Some(4));
        assert_eq!(table.ancestor_named(7, "omnix"), Some(5));
        assert_eq!(table.ancestor_named(4, "omnix"), None);
        assert_eq!(table.ancestor_named(5, "omnix"), None);
        assert_eq!(table.ancestor_named(2, "omnix"), Some(1));
        assert_eq!(table.ancestor_named(99, "omnix"), None);
    }
}
