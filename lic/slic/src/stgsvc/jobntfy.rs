// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QIBM_QWT_JOBNOTIFY` (`NTFY0100`): the data queues registered on the
//! point get an entry when a job starts, ends or is placed on a job
//! queue (`docs/HOSTSVR.md` § Security and work management points). A
//! registration's "program" is the data queue and its data says which
//! notifications (`0001` to `0007`) for which subsystem (`*ANY` for
//! all). The entries are IBM's 144-byte layouts, keyed `0001` start,
//! `0002` end, `0004` job queue, cut to the queue's entry length.

use regfac::formats::wm::{NTFY_DATA, NTFY_JOB, NTFY_JOBQ};
use regfac::spec::read;
use store::obj::{NameContext, dtaq, types};

use crate::ipl::system::{System, now_unix};

/// The point and its format.
const POINT: (&str, &str) = ("QIBM_QWT_JOBNOTIFY", "NTFY0100");

/// What happened to the job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Start,
    End,
    Queued,
}

impl Event {
    /// The data queue key.
    fn key(self) -> &'static str {
        match self {
            Self::Start => "0001",
            Self::End => "0002",
            Self::Queued => "0004",
        }
    }

    /// Whether a registration's notification type asks for it.
    fn wanted(self, kind: &str) -> bool {
        let n: u8 = kind.trim().parse().unwrap_or(0);
        match self {
            Self::Start => matches!(n, 1 | 3 | 5 | 7),
            Self::End => matches!(n, 2 | 3 | 6 | 7),
            Self::Queued => matches!(n, 4..=7),
        }
    }
}

/// One job as the entry describes it.
pub struct Job<'a> {
    /// Name, user, number.
    pub name: &'a str,
    /// User.
    pub user: &'a str,
    /// Number.
    pub number: u32,
    /// `lib/name` of the job queue, blank for none.
    pub jobq: &'a str,
    /// The subsystem, blank for none.
    pub subsystem: &'a str,
    /// Type and subtype.
    pub kind: (char, char),
    /// Unix times: entered, started, ended (0 for none).
    pub times: (u64, u64, u64),
    /// End severity and processor milliseconds (end only).
    pub end: (i32, u64),
}

fn dts(t: u64) -> [u8; 8] {
    if t == 0 {
        return [0; 8];
    }
    sysapi::to_dts(sysapi::Civil::of_unix(t))
}

fn qualified_job(j: &Job<'_>) -> Vec<u8> {
    let mut b = regfac::text::fixed(j.name, 10);
    b.extend(regfac::text::fixed(j.user, 10));
    b.extend(regfac::text::fixed(&format!("{:06}", j.number), 6));
    b
}

fn qualified_queue(q: &str) -> Vec<u8> {
    if q.trim().is_empty() {
        return vec![0; 20];
    }
    let (lib, name) = q.split_once('/').unwrap_or(("", q));
    let mut b = regfac::text::fixed(name, 10);
    b.extend(regfac::text::fixed(lib, 10));
    b
}

fn ch(c: char) -> [u8; 1] {
    [ccsid::byte_of(c).unwrap_or(0x40)]
}

/// The entry for `e`.
fn entry(e: Event, j: &Job<'_>) -> Vec<u8> {
    let mut id = [0u8; 16];
    id[..4].copy_from_slice(&j.number.to_be_bytes());
    if e == Event::Queued {
        let mut r = NTFY_JOBQ.zeroed();
        r.text("MSGID", "*JOBNOTIFY")
            .text("MSGFMT", "02")
            .bytes("INTJOBID", &id)
            .bytes("JOB", &qualified_job(j))
            .bytes("JOBQ", &qualified_queue(j.jobq))
            .bytes("ENTERED", &dts(j.times.0))
            .bytes("TYPE", &ch(j.kind.0))
            .bytes("SUBTYPE", &ch(j.kind.1))
            .bytes("JOBQASP", &regfac::text::fixed("*SYSBAS", 10));
        return r.into_bytes();
    }
    let mut r = NTFY_JOB.zeroed();
    r.text("MSGID", "*JOBNOTIFY")
        .text("MSGFMT", "01")
        .bytes("INTJOBID", &id)
        .bytes("JOB", &qualified_job(j))
        .bytes("JOBQ", &qualified_queue(j.jobq))
        .bytes("ENTERED", &dts(j.times.0))
        .bytes("STARTED", &dts(j.times.1))
        .bytes("ENDED", &dts(j.times.2))
        .bytes("TYPE", &ch(j.kind.0))
        .bytes("SUBTYPE", &ch(j.kind.1))
        .bin4("ENDSEV", if e == Event::End { j.end.0 } else { 0 })
        .bytes(
            "CPUMS",
            &if e == Event::End { j.end.1 } else { 0 }.to_be_bytes(),
        )
        .bytes("JOBQASP", &regfac::text::fixed("*SYSBAS", 10));
    r.into_bytes()
}

/// Put the entry for `e` on every data queue registered for it.
fn notify(system: &mut System, e: Event, j: &Job<'_>) {
    let data = entry(e, j);
    let sbs = j.subsystem.trim().to_ascii_uppercase();
    let r = super::spool::with_store(system, |txn, qsys| {
        let Ok(reg) = regfac::store::load(txn, qsys) else {
            return Ok(0);
        };
        let Some(point) = reg.point(POINT.0, POINT.1) else {
            return Ok(0);
        };
        let mut sent = 0;
        for p in &point.programs {
            let kind = regfac::text::field(read(&NTFY_DATA, &p.data, "NTFYTYPE"));
            let want_sbs = regfac::text::field(read(&NTFY_DATA, &p.data, "SBSD"));
            if !e.wanted(&kind) {
                continue;
            }
            if !(want_sbs.is_empty() || want_sbs == "*ANY" || want_sbs == sbs) {
                continue;
            }
            let lib = if p.lib.is_empty() {
                "*LIBL"
            } else {
                p.lib.as_str()
            };
            let found = sec::guard::as_system(|| {
                wm::obj::find(txn, qsys, &NameContext::default(), lib, &p.pgm, types::DTAQ)
            });
            let Ok(id) = found else {
                continue;
            };
            let sent_one = sec::guard::as_system(|| {
                let attr = dtaq::describe(txn, id)?;
                let mut d = data.clone();
                d.truncate(attr.max_entry_len as usize);
                dtaq::send(txn, id, &regfac::text::ebcdic(e.key()), &d, None)
            });
            if sent_one.is_ok() {
                sent += 1;
            }
        }
        Ok(sent)
    });
    if let Err(e) = r {
        eprintln!("qsstg: job notification: {e}");
    }
}

/// A job started.
pub fn started(system: &mut System, j: &wm::table::JobInfo, entered: u64, jobq: &str) {
    notify(
        system,
        Event::Start,
        &Job {
            name: &j.name,
            user: &j.user,
            number: j.number,
            jobq,
            subsystem: &j.subsystem,
            kind: (j.kind, j.subtype),
            times: (if entered == 0 { j.started } else { entered }, j.started, 0),
            end: (0, 0),
        },
    );
}

/// A job ended with end severity `code`, having used `cpu_ms`.
pub fn ended(system: &mut System, j: &wm::table::JobInfo, code: u16, cpu_ms: u64) {
    notify(
        system,
        Event::End,
        &Job {
            name: &j.name,
            user: &j.user,
            number: j.number,
            jobq: "",
            subsystem: &j.subsystem,
            kind: (j.kind, j.subtype),
            times: (j.started, j.started, now_unix()),
            end: (i32::from(code), cpu_ms),
        },
    );
}

/// A job was placed on job queue `jobq` (`lib/name`).
pub fn queued(system: &mut System, (name, user, number): (&str, &str, u32), jobq: &str) {
    let now = now_unix();
    notify(
        system,
        Event::Queued,
        &Job {
            name,
            user,
            number,
            jobq,
            subsystem: "",
            kind: ('B', ' '),
            times: (now, 0, 0),
            end: (0, 0),
        },
    );
}
