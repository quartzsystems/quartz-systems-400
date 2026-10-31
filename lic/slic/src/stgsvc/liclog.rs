// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service's event log: the Licensed Internal Code log SST
//! shows beside the IPL history. Each entry has a major and a minor
//! code in IBM's four-hex-digit form; the codes are Quartz's and
//! `docs/SERVICE.md` lists them. The newest [`KEEP`] entries are kept
//! in memory for the life of the service.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Entries kept.
pub const KEEP: usize = 2000;

/// The major codes: what part of the system wrote the entry.
pub const MAJORS: [(u16, &str); 8] = [
    (0x0100, "IPL and power"),
    (0x0200, "Storage service"),
    (0x0300, "Jobs"),
    (0x0400, "Problem log"),
    (0x0500, "Dumps"),
    (0x0600, "Traces"),
    (0x0700, "Service tools"),
    (0x0800, "Disk units"),
];

/// The minor codes under each major.
pub const MINORS: [(u16, u16, &str); 18] = [
    (0x0100, 0x0001, "Storage service started"),
    (0x0100, 0x0002, "Storage service closing"),
    (0x0200, 0x0001, "Request refused"),
    (0x0300, 0x0001, "Job started"),
    (0x0300, 0x0002, "Job ended"),
    (0x0300, 0x0003, "Job ended abnormally"),
    (0x0400, 0x0001, "Problem logged"),
    (0x0400, 0x0002, "Problem reported"),
    (0x0500, 0x0001, "System dump taken"),
    (0x0500, 0x0002, "System dump deleted"),
    (0x0500, 0x0003, "Object or job dump printed"),
    (0x0600, 0x0001, "Trace started"),
    (0x0600, 0x0002, "Trace ended"),
    (0x0700, 0x0001, "Service tools user signed on"),
    (0x0700, 0x0002, "Storage altered"),
    (0x0700, 0x0003, "Service tools user signed off"),
    (0x0800, 0x0001, "Disk configuration changed"),
    (0x0800, 0x0002, "Disk unit not reporting"),
];

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Sequence number from 1.
    pub seq: u64,
    /// Unix seconds.
    pub time: u64,
    /// Major code.
    pub major: u16,
    /// Minor code.
    pub minor: u16,
    /// Job number, 0 for the system.
    pub job: u32,
    /// Detail.
    pub text: String,
}

struct Log {
    next: u64,
    entries: VecDeque<Entry>,
}

static LOG: Mutex<Log> = Mutex::new(Log {
    next: 1,
    entries: VecDeque::new(),
});

fn log() -> std::sync::MutexGuard<'static, Log> {
    LOG.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Write an entry.
pub fn write(major: u16, minor: u16, job: u32, text: &str) {
    let mut l = log();
    let seq = l.next;
    l.next += 1;
    if l.entries.len() >= KEEP {
        l.entries.pop_front();
    }
    l.entries.push_back(Entry {
        seq,
        time: crate::ipl::system::now_unix(),
        major,
        minor,
        job,
        text: text.chars().take(120).collect(),
    });
    drop(l);
    super::trace::internal("LOG", job, &format!("{major:04X}/{minor:04X} {text}"));
}

/// The entries with major code `major` (0 for all), oldest first.
pub fn entries(major: u16) -> Vec<Entry> {
    log()
        .entries
        .iter()
        .filter(|e| major == 0 || e.major == major)
        .cloned()
        .collect()
}

/// The text of a minor code.
pub fn minor_text(major: u16, minor: u16) -> &'static str {
    MINORS
        .iter()
        .find(|m| m.0 == major && m.1 == minor)
        .map_or("", |m| m.2)
}

/// An entry as a display line.
pub fn line(e: &Entry) -> String {
    let (y, mo, d, hh, mm, ss) = crate::platform::rtc::unix_to_civil(e.time);
    format!(
        "{:>6} {y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02} {:04X} {:04X} {:06} {}",
        e.seq,
        e.major,
        e.minor,
        e.job,
        if e.text.is_empty() {
            minor_text(e.major, e.minor).to_string()
        } else {
            e.text.clone()
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_majors_known() {
        for (i, a) in MINORS.iter().enumerate() {
            assert!(MAJORS.iter().any(|m| m.0 == a.0), "{a:?}");
            assert!(!MINORS[i + 1..].iter().any(|b| b.0 == a.0 && b.1 == a.1), "{a:?}");
        }
    }
}
