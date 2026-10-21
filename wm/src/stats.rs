// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the work management displays measure, read from Linux: a
//! process's CPU time, threads, priority and resident set
//! (`/proc/<pid>/stat`), the machine's CPU time (`/proc/stat`) and
//! memory (`/proc/meminfo`), a pool's page faults (its cgroup's
//! `memory.stat`), and a disk unit's requests (`/sys/block/*/stat`).
//! The parsers are pure; the storage service reads the files.

use ipc::wire_struct;

/// A process, from `/proc/<pid>/stat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcStat {
    /// User CPU, clock ticks.
    pub utime: u64,
    /// System CPU, clock ticks.
    pub stime: u64,
    /// Kernel priority.
    pub priority: i64,
    /// Threads.
    pub threads: u32,
    /// Resident pages.
    pub rss_pages: u64,
}

impl ProcStat {
    /// CPU, clock ticks.
    #[must_use]
    pub fn cpu(&self) -> u64 {
        self.utime + self.stime
    }
}

/// Parse `/proc/<pid>/stat`. The command name may hold blanks and
/// parentheses, so fields count from the last `)`.
#[must_use]
pub fn proc_stat(text: &str) -> Option<ProcStat> {
    let rest = &text[text.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // f[0] is field 3 (state); field n is f[n - 3].
    let n = |i: usize| -> Option<i64> { f.get(i - 3)?.parse().ok() };
    Some(ProcStat {
        utime: u64::try_from(n(14)?).ok()?,
        stime: u64::try_from(n(15)?).ok()?,
        priority: n(18)?,
        threads: u32::try_from(n(20)?).ok()?,
        rss_pages: u64::try_from(n(24)?).ok()?,
    })
}

/// The machine's CPU time, clock ticks summed over every CPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    /// Every state.
    pub total: u64,
    /// Idle and waiting for I/O.
    pub idle: u64,
    /// CPUs.
    pub cpus: u32,
}

/// Parse `/proc/stat`.
#[must_use]
pub fn cpu_times(text: &str) -> Option<CpuTimes> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|w| w.parse().ok())
        .collect();
    let cpus = text
        .lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .count();
    Some(CpuTimes {
        total: v.iter().take(8).sum(),
        idle: v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0),
        cpus: u32::try_from(cpus.max(1)).unwrap_or(1),
    })
}

/// `MemTotal` in kilobytes, from `/proc/meminfo`.
#[must_use]
pub fn mem_total_kb(text: &str) -> Option<u64> {
    text.lines()
        .find(|l| l.starts_with("MemTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// A pool's page faults, from its cgroup's `memory.stat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Faults {
    /// Faults served from memory (`pgfault` less `pgmajfault`).
    pub minor: u64,
    /// Faults that read the disk (`pgmajfault`).
    pub major: u64,
}

wire_struct!(Faults { minor, major });

/// Parse a cgroup's `memory.stat`.
#[must_use]
pub fn memory_stat(text: &str) -> Faults {
    let get = |key: &str| -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(key).and_then(|v| v.trim().parse().ok()))
            .unwrap_or(0)
    };
    let all = get("pgfault ");
    let major = get("pgmajfault ");
    Faults {
        minor: all.saturating_sub(major),
        major,
    }
}

/// A disk's counters, from `/sys/block/<dev>/stat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiskStat {
    /// Reads completed.
    pub reads: u64,
    /// 512-byte sectors read.
    pub read_sectors: u64,
    /// Writes completed.
    pub writes: u64,
    /// 512-byte sectors written.
    pub write_sectors: u64,
    /// Milliseconds the device was busy.
    pub busy_ms: u64,
}

/// Parse `/sys/block/<dev>/stat` (or a partition's).
#[must_use]
pub fn block_stat(text: &str) -> Option<DiskStat> {
    let v: Vec<u64> = text
        .split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect();
    Some(DiskStat {
        reads: *v.first()?,
        read_sectors: *v.get(2)?,
        writes: *v.get(4)?,
        write_sectors: *v.get(6)?,
        busy_ms: *v.get(9)?,
    })
}

impl DiskStat {
    /// What happened since `before`.
    #[must_use]
    pub fn since(&self, before: &Self) -> Self {
        Self {
            reads: self.reads.saturating_sub(before.reads),
            read_sectors: self.read_sectors.saturating_sub(before.read_sectors),
            writes: self.writes.saturating_sub(before.writes),
            write_sectors: self.write_sectors.saturating_sub(before.write_sectors),
            busy_ms: self.busy_ms.saturating_sub(before.busy_ms),
        }
    }
}

/// `part` of `whole` as a percentage, one decimal; 0 when `whole` is 0.
#[must_use]
#[allow(clippy::cast_precision_loss)] // counters far below 2^52
pub fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 * 1000.0 / whole as f64).round() / 10.0
    }
}

/// `hh:mm:ss` of a number of seconds.
#[must_use]
pub fn hms(secs: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::many_single_char_names)] // one per file read
    fn parsers() {
        let p = proc_stat(
            "42 (qs job (x)) S 1 42 42 0 -1 4194560 300 0 0 0 7 3 0 0 20 0 2 0 1000 10000000 512 18446744073709551615",
        )
        .unwrap();
        assert_eq!(
            (p.utime, p.stime, p.priority, p.threads, p.rss_pages),
            (7, 3, 20, 2, 512)
        );
        assert_eq!(p.cpu(), 10);
        let c =
            cpu_times("cpu  10 0 5 80 5 0 0 0 0 0\ncpu0 5 0 2 40 3 0 0 0\ncpu1 5 0 3 40 2 0 0 0\n")
                .unwrap();
        assert_eq!((c.total, c.idle, c.cpus), (100, 85, 2));
        assert_eq!(
            mem_total_kb("MemTotal:        2048000 kB\nMemFree: 1 kB\n"),
            Some(2_048_000)
        );
        let f = memory_stat("anon 1\npgfault 120\npgmajfault 20\n");
        assert_eq!((f.minor, f.major), (100, 20));
        let d = block_stat("  100 0 800 50 40 0 320 30 0 70 80 0 0 0 0").unwrap();
        assert_eq!(
            (
                d.reads,
                d.read_sectors,
                d.writes,
                d.write_sectors,
                d.busy_ms
            ),
            (100, 800, 40, 320, 70)
        );
        let e = d.since(&DiskStat {
            reads: 50,
            ..DiskStat::default()
        });
        assert_eq!(e.reads, 50);
        assert!((percent(1, 3) - 33.3).abs() < 1e-9);
        assert_eq!(hms(3725), "01:02:05");
    }
}
