// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job log: every message the job's queues receive, every request
//! it runs, kept in order with the request each message belongs to.
//! What reaches the printed log (`QPJOBLOG`) is decided when it is
//! printed, by `LOG(level severity text)`:
//!
//! | Level | Logged |
//! |---|---|
//! | 0 | nothing |
//! | 1 | messages to the external queue at or above the severity |
//! | 2 | level 1, and each request that has a message at or above the severity, with all its messages |
//! | 3 | level 1, and every request, with all messages of requests that have one at or above the severity |
//! | 4 | every request, and every message at or above the severity |
//!
//! `*MSG` prints first-level text, `*SECLVL` both levels, and
//! `*NOLIST` prints nothing unless the job ended abnormally, when it
//! prints as `*SECLVL`.

use std::collections::VecDeque;
use std::fmt::Write as _;

use ipc::wire_struct;

/// One logged message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// `AAAnnnn`, blank for a request.
    pub msgid: String,
    /// `Escape`, `Diagnostic`, `Information`, `Completion`, `Inquiry`,
    /// `Reply`, `Request`, `Notify`, `Status`.
    pub kind: String,
    /// Severity.
    pub severity: u16,
    /// When, microseconds since the epoch.
    pub time: u64,
    /// Sending program and its library.
    pub from: (String, String),
    /// Its statement, `*N` when unknown.
    pub from_inst: String,
    /// Receiving program and library: `*EXT` for the external queue.
    pub to: (String, String),
    /// Its statement.
    pub to_inst: String,
    /// First-level text.
    pub text: String,
    /// Second-level text.
    pub help: String,
    /// The request it belongs to: its sequence, 0 for none.
    pub request: u32,
    /// Sent to the job's external queue.
    pub external: bool,
}

wire_struct!(Entry {
    msgid,
    kind,
    severity,
    time,
    from,
    from_inst,
    to,
    to_inst,
    text,
    help,
    request,
    external
});

impl Entry {
    /// About how many bytes it takes in the job message queue.
    #[must_use]
    pub fn size(&self) -> usize {
        96 + self.text.len() + self.help.len()
    }
}

/// What a full log does (`JOBMSGQFL`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Full {
    /// `*NOWRAP`: the job ends.
    #[default]
    NoWrap,
    /// `*WRAP`: the oldest entries go.
    Wrap,
    /// `*PRTWRAP`: the oldest entries are printed, then go.
    PrtWrap,
}

impl Full {
    /// From the keyword.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "*WRAP" => Self::Wrap,
            "*PRTWRAP" => Self::PrtWrap,
            _ => Self::NoWrap,
        }
    }
}

/// What appending did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Appended {
    /// It went in.
    Logged,
    /// It went in after the oldest entries went; those to print when
    /// the action is `*PRTWRAP`.
    Wrapped(Vec<Entry>),
    /// The log is full and the action is `*NOWRAP`: the job must end.
    Full,
}

/// The log of one job.
#[derive(Clone, Debug, Default)]
pub struct JobLog {
    entries: VecDeque<Entry>,
    bytes: usize,
    /// `JOBMSGQMX` in bytes.
    pub max_bytes: usize,
    /// `JOBMSGQFL`.
    pub full: Full,
    /// Requests logged so far.
    pub requests: u32,
    /// Entries that went to make room.
    pub dropped: u64,
}

impl JobLog {
    /// A log of `max_mb` megabytes.
    #[must_use]
    pub fn new(max_mb: u16, full: Full) -> Self {
        Self {
            max_bytes: usize::from(max_mb.max(2)) << 20,
            full,
            ..Self::default()
        }
    }

    /// Log a request (a command or a call the job was asked to run);
    /// later messages belong to it.
    pub fn request(&mut self, text: &str, time: u64, from: (String, String)) -> Appended {
        self.requests += 1;
        self.append(Entry {
            kind: "Request".into(),
            time,
            from,
            from_inst: "*N".into(),
            to: ("*EXT".into(), String::new()),
            to_inst: "*N".into(),
            text: text.to_string(),
            request: self.requests,
            external: true,
            ..Entry::default()
        })
    }

    /// Log a message; it belongs to the current request.
    pub fn append(&mut self, mut e: Entry) -> Appended {
        if e.request == 0 {
            e.request = self.requests;
        }
        let size = e.size();
        if self.max_bytes > 0 && self.bytes + size > self.max_bytes {
            if self.full == Full::NoWrap {
                return Appended::Full;
            }
            let mut gone = Vec::new();
            while self.bytes + size > self.max_bytes {
                let Some(old) = self.entries.pop_front() else {
                    break;
                };
                self.bytes -= old.size();
                self.dropped += 1;
                gone.push(old);
            }
            self.bytes += size;
            self.entries.push_back(e);
            return Appended::Wrapped(if self.full == Full::PrtWrap {
                gone
            } else {
                Vec::new()
            });
        }
        self.bytes += size;
        self.entries.push_back(e);
        Appended::Logged
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The entries `LOG(level severity _)` keeps, oldest first.
#[must_use]
pub fn filter<'a>(entries: &[&'a Entry], level: u8, sev: u16) -> Vec<&'a Entry> {
    let is_rqs = |e: &Entry| e.kind == "Request";
    // Requests that have a message at or above the severity.
    let hot: std::collections::BTreeSet<u32> = entries
        .iter()
        .filter(|e| !is_rqs(e) && e.severity >= sev && e.request > 0)
        .map(|e| e.request)
        .collect();
    entries
        .iter()
        .copied()
        .filter(|e| {
            let level1 = !is_rqs(e) && e.external && e.severity >= sev;
            match level {
                0 => false,
                1 => level1,
                2 => level1 || hot.contains(&e.request),
                3 => level1 || is_rqs(e) || hot.contains(&e.request),
                _ => is_rqs(e) || e.severity >= sev,
            }
        })
        .collect()
}

/// Who the printed log is for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// Job name, user and number.
    pub job: (String, String, u32),
    /// Job description and library.
    pub jobd: (String, String),
    /// System name.
    pub system: String,
    /// When it is printed, as `MM/DD/YY` and `HH:MM:SS`.
    pub stamp: (String, String),
}

/// `MM/DD/YY HH:MM:SS.uuuuuu` of a microsecond time, in UTC.
#[must_use]
pub fn stamp_of(micros: u64) -> (String, String) {
    let secs = micros / 1_000_000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, m, d) = civil(days);
    let s = secs % 86_400;
    (
        format!("{m:02}/{d:02}/{:02}", y.rem_euclid(100)),
        format!(
            "{:02}:{:02}:{:02}.{:06}",
            s / 3600,
            s % 3600 / 60,
            s % 60,
            micros % 1_000_000
        ),
    )
}

/// Days since 1970-01-01 to a civil date.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Lines on a printed page.
pub const PAGE_LINES: usize = 66;

/// The printed job log, `QPJOBLOG`'s layout: a header on every page,
/// then one block per entry: identifier, type, severity, date, time,
/// sender and receiver with their statements, then the text and, with
/// `seclvl`, the second-level text. Lines are 132 columns at most.
#[must_use]
pub fn print(h: &Header, entries: &[&Entry], seclvl: bool) -> Vec<String> {
    let mut body: Vec<String> = Vec::new();
    for e in entries {
        let (date, time) = stamp_of(e.time);
        body.push(format!(
            "{:<10} {:<13} {:02}  {date}  {time}  {:<10}  {:<10}  {:<6}  {:<10}  {:<10}  {}",
            if e.msgid.is_empty() { "" } else { &e.msgid },
            e.kind,
            e.severity,
            e.from.0,
            e.from.1,
            e.from_inst,
            e.to.0,
            e.to.1,
            e.to_inst
        ));
        wrap_into(&mut body, "Message . . . . :", &e.text);
        if seclvl && !e.help.is_empty() {
            wrap_into(&mut body, "Cause . . . . . :", &e.help);
        }
    }
    let mut out = Vec::new();
    let per_page = PAGE_LINES - 6;
    let pages = body.len().div_ceil(per_page).max(1);
    for p in 0..pages {
        out.push(format!(
            "{:<20}{:<40}{:<10}{:>10} {:>8} UTC   Page {:>4}",
            format!("QS400 {}", "V1R0M0"),
            "Display Job Log",
            h.system,
            h.stamp.0,
            h.stamp.1,
            p + 1
        ));
        out.push(format!(
            " Job name . . . . . . . . . . :   {:<10}  User  . . . . . . :   {:<10}  Number . . . . . . . . . . . :   {:06}",
            h.job.0, h.job.1, h.job.2
        ));
        out.push(format!(
            " Job description  . . . . . . :   {:<10}  Library . . . . . :   {:<10}",
            h.jobd.0, h.jobd.1
        ));
        out.push(
            "MSGID      TYPE          SEV  DATE      TIME             FROM PGM    LIBRARY     INST    TO PGM      LIBRARY     INST"
                .to_string(),
        );
        out.extend(body.iter().skip(p * per_page).take(per_page).cloned());
    }
    out
}

/// A labeled text block, wrapped to 132 columns.
fn wrap_into(out: &mut Vec<String>, label: &str, text: &str) {
    const INDENT: usize = 38;
    const WIDTH: usize = 132 - INDENT - 2;
    let mut line = String::new();
    let mut first = true;
    let mut flush = |line: &mut String, first: &mut bool| {
        let lead = if *first {
            format!("{:>w$}{label}   ", "", w = INDENT - label.len() - 3)
        } else {
            " ".repeat(INDENT)
        };
        out.push(format!("{lead}{line}").trim_end().to_string());
        line.clear();
        *first = false;
    };
    for w in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + w.len() > WIDTH {
            flush(&mut line, &mut first);
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    if !line.is_empty() || first {
        flush(&mut line, &mut first);
    }
}

/// `DSPJOBLOG` of an active job: the kept entries, one line each.
#[must_use]
pub fn display(entries: &[&Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        let (date, time) = stamp_of(e.time);
        let _ = writeln!(
            out,
            "{:<7} {:<11} {:02} {date} {}  {}",
            if e.msgid.is_empty() { ">>" } else { &e.msgid },
            e.kind,
            e.severity,
            &time[..8],
            e.text
        );
    }
    if out.is_empty() {
        out.push_str("No messages logged.\n");
    }
    out
}

/// Message data of fixed CP037 fields: `(text, length)` each.
#[must_use]
pub fn data(fields: &[(&str, usize)]) -> Vec<u8> {
    fields
        .iter()
        .flat_map(|(s, n)| crate::rec::text_bytes(s, *n))
        .collect()
}

/// `CPF1124` data: job, entered and started stamps, subsystem.
#[must_use]
pub fn started_data(job: (&str, &str, u32), subsystem: &str, now: u64, entered: u64) -> Vec<u8> {
    let (d, t) = stamp_of(now.saturating_mul(1_000_000));
    let (ed, et) = stamp_of(entered.saturating_mul(1_000_000));
    let num = format!("{:06}", job.2);
    let sbs = if subsystem.is_empty() {
        "*NONE"
    } else {
        subsystem
    };
    data(&[
        (job.0, 10),
        (job.1, 10),
        (&num, 6),
        (&d, 8),
        (&t[..8], 8),
        (sbs, 10),
        ("QSYS", 10),
        (&ed, 8),
        (&et[..8], 8),
    ])
}

/// `CPF1164` data: job, end stamp, processing seconds, end code.
#[must_use]
pub fn ended_data(job: (&str, &str, u32), now: u64, cpu_secs: u64, code: u16) -> Vec<u8> {
    let (d, t) = stamp_of(now.saturating_mul(1_000_000));
    let num = format!("{:06}", job.2);
    data(&[
        (job.0, 10),
        (job.1, 10),
        (&num, 6),
        (&d, 8),
        (&t[..8], 8),
        (&cpu_secs.to_string(), 10),
        (&code.to_string(), 3),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, sev: u16, external: bool) -> Entry {
        Entry {
            msgid: id.into(),
            kind: "Information".into(),
            severity: sev,
            time: 1_792_000_000_000_000,
            from: ("QCMD".into(), "QSYS".into()),
            from_inst: "*N".into(),
            to: ("*EXT".into(), String::new()),
            to_inst: "*N".into(),
            text: format!("Message {id}."),
            help: "Second level.".into(),
            external,
            ..Entry::default()
        }
    }

    #[test]
    fn levels() {
        let mut log = JobLog::new(16, Full::NoWrap);
        log.request("call A", 0, ("QCMD".into(), "QSYS".into()));
        log.append(msg("CPF0001", 0, false));
        log.request("call B", 0, ("QCMD".into(), "QSYS".into()));
        log.append(msg("CPF0002", 40, false));
        log.append(msg("CPF0003", 50, true));
        let all: Vec<&Entry> = log.entries().collect();
        let ids = |v: Vec<&Entry>| -> Vec<String> {
            v.iter()
                .map(|e| {
                    if e.msgid.is_empty() {
                        e.text.clone()
                    } else {
                        e.msgid.clone()
                    }
                })
                .collect()
        };
        assert!(filter(&all, 0, 0).is_empty());
        assert_eq!(ids(filter(&all, 1, 30)), ["CPF0003"]);
        assert_eq!(ids(filter(&all, 2, 30)), ["call B", "CPF0002", "CPF0003"]);
        assert_eq!(
            ids(filter(&all, 3, 30)),
            ["call A", "call B", "CPF0002", "CPF0003"]
        );
        assert_eq!(
            ids(filter(&all, 4, 30)),
            ["call A", "call B", "CPF0002", "CPF0003"]
        );
        assert_eq!(filter(&all, 4, 0).len(), 5);
    }

    #[test]
    fn full() {
        let mut log = JobLog::new(2, Full::NoWrap);
        log.max_bytes = 1000;
        let big = |n| Entry {
            text: "x".repeat(300),
            msgid: format!("CPF{n:04}"),
            ..Entry::default()
        };
        assert_eq!(log.append(big(1)), Appended::Logged);
        assert_eq!(log.append(big(2)), Appended::Logged);
        assert_eq!(log.append(big(3)), Appended::Full);
        log.full = Full::PrtWrap;
        match log.append(big(3)) {
            Appended::Wrapped(gone) => assert_eq!(gone[0].msgid, "CPF0001"),
            other => panic!("{other:?}"),
        }
        assert_eq!(log.dropped, 1);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn printed() {
        let e = msg("CPF1124", 0, true);
        let h = Header {
            job: ("QSVCJOB".into(), "QSYS".into(), 7),
            jobd: ("QDFTJOBD".into(), "QGPL".into()),
            system: "QS400".into(),
            stamp: ("10/21/26".into(), "12:00:00".into()),
        };
        let lines = print(&h, &[&e], true);
        assert!(lines[1].contains("QSVCJOB") && lines[1].contains("000007"));
        assert!(
            lines[4].starts_with("CPF1124    Information   00  10/14/26  17:46:40"),
            "{}",
            lines[4]
        );
        assert!(
            lines[5]
                .trim_start()
                .starts_with("Message . . . . :   Message CPF1124.")
        );
        assert!(lines[6].contains("Cause . . . . . :   Second level."));
        assert!(lines.iter().all(|l| l.len() <= 132));
        assert_eq!(stamp_of(0), ("01/01/70".into(), "00:00:00.000000".into()));
    }
}
