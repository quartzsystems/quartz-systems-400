// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Traces in the storage service (`docs/SERVICE.md`): job traces
//! (`STRTRC`, `TRCJOB`) of the requests a job makes, its commands, calls,
//! messages and I/O; internal traces (`TRCINT`, SST's "Trace Licensed
//! Internal Code") of the service's own trace points. A trace is a named
//! session with filters and a bounded buffer that wraps or stops when
//! full; ended traces stay until stored or deleted.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// What a trace collects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A job trace (`STRTRC`, `TRCJOB`).
    Job,
    /// The service's internal trace points.
    Internal,
}

/// A job trace's `JOBTRCTYPE`/`TRCTYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// Calls, commands and requests by name.
    Flow,
    /// Messages and I/O with their data.
    Data,
    /// Both.
    All,
}

impl Detail {
    /// From `*FLOW`, `*DATA`, `*ALL`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "*FLOW" => Self::Flow,
            "*DATA" => Self::Data,
            _ => Self::All,
        }
    }

    /// The special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Flow => "*FLOW",
            Self::Data => "*DATA",
            Self::All => "*ALL",
        }
    }
}

/// The categories of trace records.
pub const CATEGORIES: [(&str, &str); 9] = [
    ("CALL", "Program calls"),
    ("CMD", "Commands"),
    ("REQ", "Other requests"),
    ("MSG", "Messages"),
    ("IO", "File and object I/O"),
    ("JOB", "Job starts and ends"),
    ("PRB", "Problems logged"),
    ("DMP", "Dumps"),
    ("LOG", "LIC log entries"),
];

/// One record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Milliseconds since the Unix epoch.
    pub ms: u64,
    /// The job, 0 for the system.
    pub job: u32,
    /// Category from [`CATEGORIES`].
    pub category: &'static str,
    /// What happened.
    pub text: String,
}

/// One trace session.
#[derive(Debug, Clone)]
pub struct Trace {
    /// Session or table name.
    pub name: String,
    /// What it collects.
    pub kind: Kind,
    /// Jobs traced; empty for every job.
    pub jobs: Vec<u32>,
    /// Categories kept; empty for all.
    pub categories: Vec<String>,
    /// Job trace detail.
    pub detail: Detail,
    /// Records kept at most.
    pub max: usize,
    /// Wrap when full (`*WRAP`); otherwise stop (`*STOPTRC`).
    pub wrap: bool,
    /// Still collecting.
    pub active: bool,
    /// Unix seconds started.
    pub started: u64,
    /// Records lost to wrapping.
    pub lost: u64,
    /// The records.
    pub records: VecDeque<Record>,
}

impl Trace {
    /// A new active session.
    #[must_use]
    pub fn new(name: &str, kind: Kind, jobs: Vec<u32>, detail: Detail, max_kib: u64, wrap: bool) -> Self {
        // About a hundred bytes a record.
        let max = usize::try_from(max_kib.saturating_mul(10)).unwrap_or(usize::MAX).clamp(16, 1_000_000);
        Self {
            name: name.to_ascii_uppercase(),
            kind,
            jobs,
            categories: Vec::new(),
            detail,
            max,
            wrap,
            active: true,
            started: crate::ipl::system::now_unix(),
            lost: 0,
            records: VecDeque::new(),
        }
    }

    fn wants(&self, kind: Kind, job: u32, category: &str) -> bool {
        if !self.active || self.kind != kind {
            return false;
        }
        if !self.jobs.is_empty() && !self.jobs.contains(&job) {
            return false;
        }
        if !self.categories.is_empty() && !self.categories.iter().any(|c| c == category) {
            return false;
        }
        match (kind, self.detail) {
            (Kind::Job, Detail::Flow) => matches!(category, "CALL" | "CMD" | "REQ"),
            (Kind::Job, Detail::Data) => matches!(category, "MSG" | "IO"),
            _ => true,
        }
    }

    fn push(&mut self, r: Record) {
        if self.records.len() >= self.max {
            if self.wrap {
                self.records.pop_front();
                self.lost += 1;
            } else {
                self.active = false;
                return;
            }
        }
        self.records.push_back(r);
    }

    /// The trace as stored text: a header line, then a line a record.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = format!(
            "QS400TRC 1\t{}\t{}\t{}\t{}\t{}\n",
            self.name,
            if self.kind == Kind::Job { "J" } else { "I" },
            self.detail.name(),
            self.started,
            self.lost
        );
        for r in &self.records {
            let _ = writeln!(out, "{}\t{}\t{}\t{}", r.ms, r.job, r.category, r.text.replace(['\t', '\n'], " "));
        }
        out
    }

    /// From stored text.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let h: Vec<&str> = lines.next()?.split('\t').collect();
        if h.len() != 6 || !h[0].starts_with("QS400TRC") {
            return None;
        }
        let kind = if h[2] == "J" { Kind::Job } else { Kind::Internal };
        let mut t = Self::new(h[1], kind, Vec::new(), Detail::parse(h[3]), 100_000, true);
        t.active = false;
        t.started = h[4].parse().unwrap_or(0);
        t.lost = h[5].parse().unwrap_or(0);
        for l in lines {
            let f: Vec<&str> = l.splitn(4, '\t').collect();
            if f.len() != 4 {
                continue;
            }
            let category = CATEGORIES.iter().find(|c| c.0 == f[2]).map_or("REQ", |c| c.0);
            t.records.push_back(Record {
                ms: f[0].parse().unwrap_or(0),
                job: f[1].parse().unwrap_or(0),
                category,
                text: f[3].to_string(),
            });
        }
        Some(t)
    }
}

static ANY: AtomicBool = AtomicBool::new(false);
static TRACES: Mutex<BTreeMap<String, Trace>> = Mutex::new(BTreeMap::new());

fn traces() -> std::sync::MutexGuard<'static, BTreeMap<String, Trace>> {
    TRACES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn record(kind: Kind, job: u32, category: &'static str, text: &str) {
    if !ANY.load(Ordering::Relaxed) {
        return;
    }
    let mut t = traces();
    for tr in t.values_mut() {
        if tr.wants(kind, job, category) {
            tr.push(Record {
                ms: now_ms(),
                job,
                category,
                text: text.chars().take(200).collect(),
            });
        }
    }
}

fn refresh(t: &BTreeMap<String, Trace>) {
    ANY.store(t.values().any(|t| t.active), Ordering::Relaxed);
}

/// A job event.
pub fn job(job: u32, category: &'static str, text: &str) {
    record(Kind::Job, job, category, text);
}

/// An internal trace point.
pub fn internal(category: &'static str, job: u32, text: &str) {
    record(Kind::Internal, job, category, text);
}

/// A request from a job: a job trace's flow, and the internal `REQ`
/// point. Requests that carry passwords or tokens never show data.
pub fn request(number: u32, req: &stg::Request) {
    if !ANY.load(Ordering::Relaxed) {
        return;
    }
    let name = req.name();
    let category: &'static str = if ["Cmd", "Exec", "Analyze"].iter().any(|k| name.contains(k)) {
        "CMD"
    } else if name.contains("Msg") || name.contains("Joblog") {
        "MSG"
    } else if ["Rla", "Io", "Read", "Write", "Open", "Close", "File", "Mbr", "Splf", "Ifs", "Usrspc", "Dtaq", "Dtaara"]
        .iter()
        .any(|k| name.contains(k))
    {
        "IO"
    } else if name.contains("Call") || name.contains("Pgm") || name.contains("Activ") {
        "CALL"
    } else {
        "REQ"
    };
    let secret = ["Sign", "Pwd", "Password", "Token", "Host", "Sst"].iter().any(|k| name.contains(k));
    let text = if secret || matches!(category, "CALL" | "CMD" | "REQ") {
        name.to_string()
    } else {
        let mut d = format!("{req:?}");
        d.truncate(d.char_indices().nth(180).map_or(d.len(), |(i, _)| i));
        d
    };
    record(Kind::Job, number, category, &text);
    record(Kind::Internal, number, "REQ", name);
}

/// Start a session; refused when one of that name is active.
pub fn start(t: Trace) -> Result<(), String> {
    let mut all = traces();
    if all.get(&t.name).is_some_and(|o| o.active) {
        return Err(format!("CPF39D0 Trace {} is already active.", t.name));
    }
    let name = t.name.clone();
    all.insert(name.clone(), t);
    refresh(&all);
    drop(all);
    super::liclog::write(0x0600, 0x0001, 0, &format!("Trace {name} started"));
    Ok(())
}

/// End a session: it stops collecting and stays for printing or
/// storing. The records it holds.
pub fn end(name: &str) -> Result<usize, String> {
    let name = name.trim().to_ascii_uppercase();
    let mut all = traces();
    let t = all
        .get_mut(&name)
        .ok_or_else(|| format!("CPF39D1 Trace {name} not found."))?;
    t.active = false;
    let n = t.records.len();
    refresh(&all);
    drop(all);
    super::liclog::write(0x0600, 0x0002, 0, &format!("Trace {name} ended, {n} records"));
    Ok(n)
}

/// A copy of a session.
pub fn get(name: &str) -> Option<Trace> {
    traces().get(&name.trim().to_ascii_uppercase()).cloned()
}

/// Remove a session.
pub fn delete(name: &str) -> Option<Trace> {
    let mut all = traces();
    let t = all.remove(&name.trim().to_ascii_uppercase());
    refresh(&all);
    t
}

/// Every session: name, kind, active, records.
pub fn list() -> Vec<(String, Kind, bool, usize)> {
    traces()
        .values()
        .map(|t| (t.name.clone(), t.kind, t.active, t.records.len()))
        .collect()
}

/// The active job traces naming `job` (or every job).
pub fn tracing(job: u32) -> Vec<String> {
    traces()
        .values()
        .filter(|t| t.active && t.kind == Kind::Job && (t.jobs.is_empty() || t.jobs.contains(&job)))
        .map(|t| t.name.clone())
        .collect()
}

/// A trace as report lines (`PRTTRC`, the SST display): a heading,
/// then one line a record; `names` gives a job's qualified name.
pub fn report(t: &Trace, names: &dyn Fn(u32) -> String) -> Vec<String> {
    let mut out = vec![
        format!(
            "Trace {}  type {}  {}  records {}  lost to wrapping {}",
            t.name,
            if t.kind == Kind::Job { "job" } else { "internal" },
            t.detail.name(),
            t.records.len(),
            t.lost
        ),
        String::new(),
        "Time          Job                          Category  Event".into(),
    ];
    for r in &t.records {
        let secs = r.ms / 1000;
        let (_, _, _, hh, mm, ss) = crate::platform::rtc::unix_to_civil(secs);
        out.push(format!(
            "{hh:02}:{mm:02}:{ss:02}.{:03}  {:<28} {:<9} {}",
            r.ms % 1000,
            names(r.job),
            r.category,
            r.text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_wrap_and_round_trip() {
        let mut t = Trace::new("t1", Kind::Job, vec![7], Detail::Flow, 0, true);
        assert_eq!(t.max, 16);
        assert!(t.wants(Kind::Job, 7, "CMD"));
        assert!(!t.wants(Kind::Job, 8, "CMD"));
        assert!(!t.wants(Kind::Job, 7, "MSG"));
        assert!(!t.wants(Kind::Internal, 7, "CMD"));
        for i in 0..20 {
            t.push(Record {
                ms: i,
                job: 7,
                category: "CMD",
                text: format!("r{i}"),
            });
        }
        assert_eq!((t.records.len(), t.lost), (16, 4));
        let back = Trace::decode(&t.encode()).unwrap();
        assert_eq!(back.records, t.records);
        assert_eq!(back.detail, Detail::Flow);
        let mut s = Trace::new("t2", Kind::Internal, Vec::new(), Detail::All, 0, false);
        for i in 0..20 {
            s.push(Record {
                ms: i,
                job: 0,
                category: "LOG",
                text: String::new(),
            });
        }
        assert!(!s.active);
        assert_eq!(s.records.len(), 16);
    }
}
