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
