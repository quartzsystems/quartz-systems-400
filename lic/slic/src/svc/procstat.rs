// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `procstat`: the guest's processes with their resident memory, open
//! descriptors and, for a job's process, the qualified job name; and a
//! closer look at one process's descriptors (`fds PID`) and memory
//! (`maps PID`), for the soak's leak checks.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ipl::system::System;

/// `procstat [fds PID | maps PID]`.
pub fn command(system: &mut System, rest: &[&str]) -> String {
    match rest {
        [] => list(system),
        ["fds", pid] => pid.parse().map_or_else(|_| usage(), fds),
        ["maps", pid] => pid.parse().map_or_else(|_| usage(), maps),
        _ => usage(),
    }
}

fn usage() -> String {
    "Usage: procstat [fds PID | maps PID]\n".into()
}

/// Every process: PID, RSS kB, descriptors, command line and the job.
fn list(system: &mut System) -> String {
    // A process holding several jobs (a server's sessions) is named by
    // its own, the one that started first.
    let mut active = system
        .procs
        .stg
        .as_mut()
        .and_then(|c| stg::Client::active_jobs(c).ok())
        .unwrap_or_default();
    active.sort_by_key(|j| (j.started, j.number));
    let mut jobs: BTreeMap<u32, String> = BTreeMap::new();
    for j in active.into_iter().filter(|j| j.pid > 1) {
        jobs.entry(j.pid).or_insert_with(|| j.qualified());
    }
    let mut out = String::from("  PID   RSS_KB    FDS  COMMAND\n");
    for pid in pids() {
        let base = format!("/proc/{pid}");
        let Ok(cmd) = std::fs::read(format!("{base}/cmdline")) else {
            continue;
        };
        if cmd.is_empty() {
            continue;
        }
        let cmd: String = String::from_utf8_lossy(&cmd).replace('\0', " ").trim().to_string();
        let rss = status_kb(pid, "VmRSS:");
        let fds = std::fs::read_dir(format!("{base}/fd")).map_or(0, Iterator::count);
        let job = jobs.get(&pid).map_or(String::new(), |j| format!(" {j}"));
        let _ = writeln!(out, "{pid:>5} {rss:>8} {fds:>6}  {cmd}{job}");
    }
    out
}

fn pids() -> Vec<u32> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids: Vec<u32> = dir
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse().ok()))
        .collect();
    pids.sort_unstable();
    pids
}

/// A `/proc/PID/status` field in kB.
fn status_kb(pid: u32, field: &str) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|n| n.parse::<u64>().ok()))
        })
        .unwrap_or(0)
}

/// A descriptor's kind: `socket`, `pipe`, `anon:NAME`, or its path.
fn kind(target: &str) -> String {
    if let Some(k) = target.strip_suffix(']').and_then(|t| t.split_once(":[")).map(|p| p.0) {
        return k.to_string();
    }
    if let Some(a) = target.strip_prefix("anon_inode:") {
        return format!("anon:{}", a.trim_start_matches('[').trim_end_matches(']'));
    }
    target.to_string()
}

/// `fds PID`: the process's descriptors counted by kind, most first.
fn fds(pid: u32) -> String {
    let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return format!("no process {pid}\n");
    };
    let mut kinds: BTreeMap<String, u32> = BTreeMap::new();
    let mut total = 0;
    for e in dir.filter_map(Result::ok) {
        total += 1;
        let t = std::fs::read_link(e.path()).map_or_else(|_| "?".into(), |p| p.to_string_lossy().into_owned());
        *kinds.entry(kind(&t)).or_default() += 1;
    }
    let mut v: Vec<(String, u32)> = kinds.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out = format!("pid {pid}: {total} descriptor(s)\n");
    for (k, n) in v {
        let _ = writeln!(out, "{n:>6}  {k}");
    }
    out
}

/// `maps PID`: resident kB by mapping (`[heap]`, anonymous, each file),
/// most first, from `smaps`.
fn maps(pid: u32) -> String {
    let Ok(t) = std::fs::read_to_string(format!("/proc/{pid}/smaps")) else {
        return format!("no process {pid}\n");
    };
    let mut by: BTreeMap<String, (u64, u32)> = BTreeMap::new();
    let mut current = String::new();
    for l in t.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        if w.first().is_some_and(|f| f.contains('-') && !f.ends_with(':')) {
            current = w.get(5).map_or_else(|| "[anon]".to_string(), |n| (*n).to_string());
            by.entry(current.clone()).or_default().1 += 1;
        } else if w.first() == Some(&"Rss:") {
            by.entry(current.clone()).or_default().0 += w.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
        }
    }
    let mut v: Vec<(String, (u64, u32))> = by.into_iter().filter(|(_, (kb, _))| *kb > 0).collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1.0));
    let mut out = format!("pid {pid}: rss {} kB\n", status_kb(pid, "VmRSS:"));
    for (name, (kb, n)) in v.into_iter().take(12) {
        let _ = writeln!(out, "{kb:>9} kB {n:>5} map(s)  {name}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kind;

    #[test]
    fn kinds() {
        assert_eq!(kind("socket:[1234]"), "socket");
        assert_eq!(kind("pipe:[99]"), "pipe");
        assert_eq!(kind("anon_inode:[eventfd]"), "anon:eventfd");
        assert_eq!(kind("/dev/null"), "/dev/null");
    }
}
