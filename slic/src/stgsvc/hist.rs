// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The history log: messages sent to `QHST` go into physical files
//! `QSYS/QHSTyydddn`, one record each, in the `QAHST`-shaped format
//! below. A file holds `QHSTLOGSIZ` records; the next one starts then,
//! or when the day changes. `DSPLOG` reads them back in order.

use std::fmt::Write as _;

use db::file::{self, CreateFileOpts, Size};
use db::odp::{Odp, OpenOpts, Usage};
use db::{Member, Value};
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use qdecimal::Decimal;
use qsname::Name;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::System;

/// The record format, one DDS line per record.
const PF: &[&str] = &[
    "     A          R QHSTREC                   TEXT('History log entry')",
    "     A            QHDATE         7A         TEXT('Date, CYYMMDD')",
    "     A            QHTIME         6A         TEXT('Time, HHMMSS')",
    "     A            QHJOB         10A         TEXT('Job name')",
    "     A            QHUSER        10A         TEXT('User')",
    "     A            QHNBR          6A         TEXT('Job number')",
    "     A            QHMSGID        7A         TEXT('Message identifier')",
    "     A            QHSEV          2S 0       TEXT('Severity')",
    "     A            QHTYPE         2A         TEXT('Message type')",
    "     A            QHMSGF        10A         TEXT('Message file')",
    "     A            QHMSGL        10A         TEXT('Message file library')",
    "     A            QHDTALEN       5S 0       TEXT('Message data length')",
    "     A            QHDTA        512A         TEXT('Message data')",
    "     A            QHTEXT       256A         TEXT('Message text')",
];

/// What goes into the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hist {
    /// Seconds since the epoch.
    pub time: u64,
    /// Identifier, blank for immediate text.
    pub msgid: String,
    /// Severity.
    pub severity: u8,
    /// Type: `01` completion, `02` diagnostic, `04` information, `05`
    /// inquiry, `08` request, `14` notify, `15` escape, `21` reply.
    pub kind: &'static str,
    /// The job: name, user, number.
    pub job: (String, String, u32),
    /// Message file and library.
    pub msgf: (String, String),
    /// Message data.
    pub data: Vec<u8>,
    /// Text.
    pub text: String,
}

/// `(CYYMMDD, HHMMSS)` of a Unix time.
fn stamp(secs: u64) -> (String, String) {
    let (d, t) = wm::joblog::stamp_of(secs.saturating_mul(1_000_000));
    // d is MM/DD/YY.
    let c = if secs >= 946_684_800 { '1' } else { '0' };
    (
        format!("{c}{}{}{}", &d[6..8], &d[0..2], &d[3..5]),
        t[..8].replace(':', ""),
    )
}

/// `yyddd` of a Unix time.
fn julian(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let year_start = |y: i64| -> i64 {
        let y1 = y - 1;
        365 * (y - 1970) + (y1 / 4 - 1969 / 4) - (y1 / 100 - 1969 / 100) + (y1 / 400 - 1969 / 400)
    };
    let mut y = 1970 + days / 366;
    while year_start(y + 1) <= days {
        y += 1;
    }
    format!("{:02}{:03}", y.rem_euclid(100), days - year_start(y) + 1)
}

fn ebc(s: &str, n: usize) -> Value {
    Value::Char(wm::rec::text_bytes(s, n))
}

fn num(n: u64) -> Value {
    Value::Num(Decimal::new(i128::from(n), 0).expect("small"))
}

/// The history files in `QSYS`, oldest first.
fn files(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<(String, ObjectId)>, String> {
    let mut v: Vec<(String, ObjectId)> = ops::list(txn, qsys, Some(types::FILE))
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(k, id)| {
            let n = k.name.to_string();
            (n.len() == 10 && n.starts_with("QHST") && n[4..].bytes().all(|b| b.is_ascii_digit()))
                .then_some((n, id))
        })
        .collect();
    v.sort();
    Ok(v)
}

/// Create `QSYS/name`.
fn create(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, now: u64) -> Result<ObjectId, String> {
    let (src, _) = Source::from_text(&PF.join("\n"));
    let comp = compile_pf(&src, &NoRefs, &CompileOpts::default());
    let def = comp
        .definition
        .ok_or_else(|| format!("history file:\n{}", comp.listing))?;
    let opts = CreateFileOpts {
        now,
        size: Size::NOMAX,
        text: "History log".into(),
        ..CreateFileOpts::default()
    };
    file::create_pf(
        txn,
        qsys,
        Name::parse(name).map_err(|e| e.to_string())?,
        &def,
        &opts,
    )
    .map_err(|e| e.to_string())
}

/// The file to write to: the newest when it is today's and has room,
/// else a new one.
fn current(
    txn: &mut Txn<'_>,
    system_qsys: ObjectId,
    limit: u64,
    now: u64,
    locks: &db::odp::SharedLocks,
) -> Result<ObjectId, String> {
    let today = julian(now);
    let all = files(txn, system_qsys)?;
    if let Some((name, id)) = all.last()
        && name[4..9] == today
    {
        let o = Odp::open(txn, locks.clone(), *id, OpenOpts::input(JobHandle(0)))
            .map_err(|e| e.to_string())?;
        let used = o.member().attr().record_count;
        o.close(txn);
        if used < limit {
            return Ok(*id);
        }
    }
    let seq = all
        .iter()
        .filter(|(n, _)| n[4..9] == today)
        .filter_map(|(n, _)| n[9..].parse::<u32>().ok())
        .max()
        .map_or(0, |s| s + 1);
    if seq > 9 {
        // Ten files a day at most: the last takes the rest.
        return Ok(all.last().expect("ten today").1);
    }
    create(txn, system_qsys, &format!("QHST{today}{seq}"), now)
}

fn fill(m: &Member, h: &Hist) -> Result<(Vec<u8>, Vec<u8>), String> {
    let fmt = m.format();
    let mut rec = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
    let nulls = vec![0u8; usize::from(fmt.null_map_len)];
    let (date, time) = stamp(h.time);
    let values = [
        ("QHDATE", ebc(&date, 7)),
        ("QHTIME", ebc(&time, 6)),
        ("QHJOB", ebc(&h.job.0, 10)),
        ("QHUSER", ebc(&h.job.1, 10)),
        ("QHNBR", ebc(&format!("{:06}", h.job.2), 6)),
        ("QHMSGID", ebc(&h.msgid, 7)),
        ("QHSEV", num(u64::from(h.severity))),
        ("QHTYPE", ebc(h.kind, 2)),
        ("QHMSGF", ebc(&h.msgf.0, 10)),
        ("QHMSGL", ebc(&h.msgf.1, 10)),
        ("QHDTALEN", num(h.data.len().min(512) as u64)),
        ("QHDTA", {
            let mut b = h.data.clone();
            b.resize(512, 0x40);
            Value::Char(b)
        }),
        ("QHTEXT", ebc(&h.text, 256)),
    ];
    for (name, v) in values {
        let f = fmt
            .format
            .fields
            .iter()
            .find(|f| f.name.to_string() == name)
            .ok_or_else(|| format!("history format has no {name}"))?;
        db::record::write_field(f, &mut rec, &v).map_err(|e| e.to_string())?;
    }
    Ok((rec, nulls))
}

/// Append to the history log.
pub fn log(system: &mut System, h: &Hist) -> Result<(), String> {
    let limit = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QHSTLOGSIZ").ok())
        .and_then(|v| v.text().trim().parse().ok())
        .unwrap_or(5000);
    let locks = system.locks.clone();
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let id = current(txn, qsys, limit, h.time, &locks)?;
        let mut o = Odp::open(
            txn,
            locks.clone(),
            id,
            OpenOpts {
                usage: Usage::OUTPUT,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .map_err(|e| e.to_string())?;
        let (rec, nulls) = fill(o.member(), h)?;
        o.write(txn, &rec, &nulls).map_err(|e| e.to_string())?;
        o.close(txn);
        Ok(())
    })
}

/// Log, reporting a failure on the serial line rather than to the
/// caller: the history log never stops what is being logged.
pub fn note(system: &mut System, h: &Hist) {
    if let Err(e) = log(system, h) {
        eprintln!("qsstg: history log: {e}");
    }
}

/// One record read back.
#[derive(Clone, Debug, Default)]
pub struct Read {
    /// `CYYMMDD`.
    pub date: String,
    /// `HHMMSS`.
    pub time: String,
    /// Job name, user, number.
    pub job: (String, String, String),
    /// Identifier.
    pub msgid: String,
    /// Severity.
    pub severity: String,
    /// Type code.
    pub kind: String,
    /// Text.
    pub text: String,
}

/// Every record of every history file, oldest first.
pub fn read_all(system: &mut System) -> Result<Vec<Read>, String> {
    let locks = system.locks.clone();
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let mut out = Vec::new();
        for (_, id) in files(txn, qsys)? {
            let mut o = Odp::open(txn, locks.clone(), id, OpenOpts::input(JobHandle(0)))
                .map_err(|e| e.to_string())?;
            while let Some(r) = o.read(txn, false).map_err(|e| e.to_string())? {
                let text = |n: &str| match o.field(&r, n) {
                    Ok(Value::Char(b)) => wm::rec::text_of(&b),
                    Ok(Value::Num(d)) => d.to_string(),
                    _ => String::new(),
                };
                out.push(Read {
                    date: text("QHDATE"),
                    time: text("QHTIME"),
                    job: (text("QHJOB"), text("QHUSER"), text("QHNBR")),
                    msgid: text("QHMSGID"),
                    severity: text("QHSEV"),
                    kind: text("QHTYPE"),
                    text: text("QHTEXT"),
                });
            }
            o.close(txn);
        }
        Ok(out)
    })
}

/// `DSPLOG [PERIOD((start-time start-date) (end-time end-date))]
/// [JOB(number/user/name)] [MSGID(id ...)]`: dates as `CYYMMDD` or
/// `MMDDYY`, times as `HHMMSS`.
pub fn dsplog(system: &mut System, p: &wm::cmd::Params) -> Result<String, String> {
    let all = read_all(system)?;
    let (from, to) = period(p.kw("PERIOD"))?;
    let job = p.upper("JOB");
    let ids: Vec<String> = p
        .upper("MSGID")
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let mut out = String::from("Displays the history log, oldest first.\n\n");
    let mut n = 0;
    for r in &all {
        let when = format!("{}{}", r.date, r.time);
        if from.as_ref().is_some_and(|f| when < *f) || to.as_ref().is_some_and(|t| when > *t) {
            continue;
        }
        if let Some(j) = &job {
            let full = format!("{}/{}/{}", r.job.2, r.job.1, r.job.0);
            if !(full == *j || r.job.0 == *j) {
                continue;
            }
        }
        if !ids.is_empty()
            && !ids
                .iter()
                .any(|i| r.msgid == *i || (i.ends_with("00") && r.msgid.starts_with(&i[..5])))
        {
            continue;
        }
        n += 1;
        let _ = writeln!(
            out,
            "{} {}:{}:{}  {:<7} {:>2} {:<2} {:<10} {:<10} {}  {}",
            r.date,
            &r.time[0..2],
            &r.time[2..4],
            &r.time[4..6],
            r.msgid,
            r.severity,
            r.kind,
            r.job.0,
            r.job.1,
            r.job.2,
            r.text
        );
    }
    let _ = writeln!(out, "{n} entr(ies)");
    Ok(out)
}

/// `PERIOD` as `CYYMMDDHHMMSS` bounds.
fn period(v: Option<&str>) -> Result<(Option<String>, Option<String>), String> {
    let Some(v) = v else {
        return Ok((None, None));
    };
    let parts = wm::cmd::Params::list(v);
    let bound = |part: Option<&String>, dft_time: &str| -> Result<Option<String>, String> {
        let Some(part) = part else {
            return Ok(None);
        };
        let w: Vec<&str> = part.split_whitespace().collect();
        let time = match w.first() {
            None | Some(&"*AVAIL") => dft_time.to_string(),
            Some(t) => t.to_string(),
        };
        let date = match w.get(1) {
            None | Some(&"*BEGIN" | &"*END") => return Ok(None),
            Some(&"*CURRENT") => stamp(now()).0,
            Some(d) if d.len() == 7 => (*d).to_string(),
            Some(d) if d.len() == 6 => format!("1{}{}{}", &d[4..6], &d[0..2], &d[2..4]),
            Some(d) => return Err(format!("CPD0084 PERIOD date {d}")),
        };
        Ok(Some(format!("{date}{time}")))
    };
    Ok((
        bound(parts.first(), "000000")?,
        bound(parts.get(1), "235959")?,
    ))
}

fn now() -> u64 {
    crate::ipl::system::now_unix()
}

thread_local! {
    /// The built-in message files, for the text of system messages.
    static BUILTIN: std::cell::RefCell<Option<msgf::BuiltinMsgs>> = const { std::cell::RefCell::new(None) };
}

/// Log a system message from `QCPFMSG` with its data, as `job`.
pub fn system_message(
    system: &mut System,
    msgid: &str,
    data: &[u8],
    kind: &'static str,
    job: (String, String, u32),
) {
    use msgf::MsgLookup as _;
    let (text, severity) = BUILTIN.with(|b| {
        let mut b = b.borrow_mut();
        let msgs = b.get_or_insert_with(msgf::BuiltinMsgs::new);
        msgs.describe(msgid, "QCPFMSG", "QSYS").map_or_else(
            || (format!("{msgid} {}", wm::rec::text_of(data)), 0),
            |d| (d.format(data), d.severity),
        )
    });
    note(
        system,
        &Hist {
            time: now(),
            msgid: msgid.into(),
            severity: u8::try_from(severity).unwrap_or(99),
            kind,
            job,
            msgf: ("QCPFMSG".into(), "QSYS".into()),
            data: data.to_vec(),
            text,
        },
    );
}

/// A system message's first-level text with its data.
pub fn message_text(msgid: &str, data: &[u8]) -> String {
    use msgf::MsgLookup as _;
    BUILTIN.with(|b| {
        let mut b = b.borrow_mut();
        let msgs = b.get_or_insert_with(msgf::BuiltinMsgs::new);
        msgs.describe(msgid, "QCPFMSG", "QSYS").map_or_else(
            || format!("{msgid} {}", wm::rec::text_of(data)),
            |d| d.format(data),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        // 2026-10-21 00:00:00 UTC.
        assert_eq!(stamp(1_792_540_800), ("1261021".into(), "000000".into()));
        assert_eq!(julian(1_792_540_800), "26294");
        assert_eq!(julian(0), "70001");
        assert_eq!(julian(951_782_400), "00060"); // 2000-02-29
    }
}
