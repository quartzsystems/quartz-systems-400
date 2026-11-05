// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DISPLAY_JOURNAL` and the `SYSTOOLS.AUDIT_JOURNAL_xx` table
//! functions: journal entries as `DSPJRN` reads them (`jrn`), the audit
//! entries' own fields from the layouts `sec::audit` writes. Remote
//! journals, threads, ASPs other than 1 and syslog forms do not exist
//! here: those columns are null and `GENERATE_SYSLOG` and `EOF_DELAY`
//! are refused (SQL0443).

use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// `*JRN`.
const JRN: u16 = 0x0018;

fn refused(what: &str) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what: format!("{what} is not supported") }
}

/// Unix seconds of a timestamp argument; `None` when null.
fn unix(v: &Value) -> Result<Option<u64>> {
    if v.is_null() {
        return Ok(None);
    }
    let ts = v.as_timestamp()?;
    let (y, m, d) = (i64::from(ts.date.year), i64::from(ts.date.month), i64::from(ts.date.day));
    // Days from 1970-01-01 of a proleptic Gregorian date.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + i64::from(ts.time.hour) * 3600 + i64::from(ts.time.minute) * 60 + i64::from(ts.time.second);
    Ok(u64::try_from(secs).ok())
}

/// A sequence number argument.
fn seq(args: &Args, name: &str) -> Option<u64> {
    args.int(name).and_then(|n| u64::try_from(n).ok())
}

/// The receivers of `jrn` the range names, in chain order:
/// `*CURRENT` the attached one, `*CURCHAIN`/`*CURAVLCHN` the chain, or
/// named ones (library `*CURRENT` or `*LIBL` meaning the journal's).
fn receivers(
    txn: &mut Txn<'_>,
    j: &jrn::journal::Journal,
    from: (&str, &str),
    to: (&str, &str),
) -> Result<Vec<jrn::rcv::Receiver>> {
    let chain = j.chain(txn).map_err(|e| SqlError::Database { what: e.to_string(), msgid: None })?;
    let find = |txn: &mut Txn<'_>, (lib, name): (&str, &str)| -> Option<usize> {
        chain.iter().position(|r| {
            r.header.name.to_string() == name
                && (matches!(lib, "*CURRENT" | "*LIBL" | "")
                    || ops::read_header(txn, r.header.library).is_ok_and(|(_, h)| h.name.to_string() == lib))
        })
    };
    let attached = chain.iter().position(|r| r.id == j.attr.attached).unwrap_or(chain.len().saturating_sub(1));
    let first = match from.1 {
        "*CURRENT" => attached,
        "*CURCHAIN" | "*CURAVLCHN" => 0,
        _ => find(txn, from).ok_or_else(|| refused(&format!("receiver {}", from.1)))?,
    };
    let last = match to.1 {
        "*CURRENT" | "" => attached,
        _ => find(txn, to).ok_or_else(|| refused(&format!("receiver {}", to.1)))?,
    };
    Ok(chain.into_iter().enumerate().filter(|(i, _)| *i >= first && *i <= last).map(|(_, r)| r).collect())
}

/// One entry with the receiver holding it.
struct Found {
    entry: jrn::Entry,
    receiver: (String, String),
}

/// The entries of journal `id` in the receivers named that pass
/// `filter` and `keep`.
fn walk(
    txn: &mut Txn<'_>,
    id: ObjectId,
    from: (&str, &str),
    to: (&str, &str),
    filter: &jrn::Filter,
    keep: &dyn Fn(&jrn::Entry) -> bool,
) -> Result<Vec<Found>> {
    let j = jrn::journal::Journal::load(txn, id).map_err(|e| SqlError::Database { what: e.to_string(), msgid: None })?;
    let mut out = Vec::new();
    for r in receivers(txn, &j, from, to)? {
        let lib = ops::read_header(txn, r.header.library).map(|(_, h)| h.name.to_string()).unwrap_or_default();
        let Ok(entries) = r.entries(txn) else { continue };
        for e in entries {
            if filter.passes(&e) && keep(&e) {
                out.push(Found { entry: e, receiver: (lib.clone(), r.header.name.to_string()) });
            }
        }
    }
    Ok(out)
}

/// The columns every entry has; those of `DISPLAY_JOURNAL` or the audit
/// functions alone are set where the shape has them.
fn common(out: &mut Out, f: &Found) {
    let e = &f.entry;
    if out.has("QUALIFIED_JOB_NAME") {
        out.text("QUALIFIED_JOB_NAME", e.job.qualified());
    }
    out.stamp_micros("ENTRY_TIMESTAMP", e.time, e.micros);
    out.dec("SEQUENCE_NUMBER", &e.seq.to_string());
    out.text("JOB_NAME", e.job.name.as_str());
    out.text("JOB_USER", e.job.user.as_str());
    out.text("JOB_NUMBER", format!("{:06}", e.job.number));
    out.text("USER_NAME", e.job.user.as_str());
    out.text_or_null("PROGRAM_NAME", &e.program);
    out.text("RECEIVER_LIBRARY", f.receiver.0.as_str());
    out.text("RECEIVER_NAME", f.receiver.1.as_str());
    out.int("RECEIVER_ASP_NUMBER", 1);
    out.text("RECEIVER_ASP_DEVICE", "*SYSBAS");
}

fn list_words(s: Option<String>) -> Vec<String> {
    s.unwrap_or_default().split([' ', ',']).filter(|w| !w.is_empty() && *w != "*ALL").map(str::to_string).collect()
}

fn name_arg(args: &Args, n: &str) -> Option<Name> {
    args.word(n).filter(|w| !w.is_empty() && !w.starts_with('*')).and_then(|w| Name::parse(&w).ok())
}

/// The job argument `number/user/name`; `None` for any.
fn job_arg(args: &Args, n: &str) -> Option<(u32, String, String)> {
    let w = args.word(n).filter(|w| !w.is_empty() && w != "*ALL")?;
    let mut p = w.split('/');
    let num = p.next()?.parse().ok()?;
    Some((num, p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string()))
}

/// `QSYS2.DISPLAY_JOURNAL`.
pub fn display(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    if !args.get("GENERATE_SYSLOG").is_null() {
        return Err(refused("GENERATE_SYSLOG"));
    }
    if !args.get("EOF_DELAY").is_null() {
        return Err(refused("EOF_DELAY"));
    }
    let lib = args.word("JOURNAL_LIBRARY").unwrap_or_default();
    let name = args.word("JOURNAL_NAME").unwrap_or_default();
    let Some(id) = super::objects::libraries(txn, ctx, &lib)?.into_iter().find_map(|l| {
        ops::list(txn, l, Some(JRN)).ok()?.into_iter().find(|(k, _)| k.name.to_string() == name).map(|(_, id)| id)
    }) else {
        return Err(SqlError::Coded { code: -443, state: "38501", what: format!("journal {lib}/{name} not found") });
    };
    let mut filter = jrn::Filter {
        codes: list_words(args.word("JOURNAL_CODES")).iter().filter_map(|c| c.bytes().next()).collect(),
        types: list_words(args.word("JOURNAL_ENTRY_TYPES"))
            .iter()
            .filter_map(|t| t.as_bytes().get(..2).map(|b| [b[0], b[1]]))
            .collect(),
        from_seq: seq(args, "STARTING_SEQUENCE"),
        to_seq: seq(args, "ENDING_SEQUENCE"),
        from_time: unix(args.get("STARTING_TIMESTAMP"))?,
        to_time: unix(args.get("ENDING_TIMESTAMP"))?,
        commit_cycle: seq(args, "COMMIT_CYCLE"),
        ..jrn::Filter::default()
    };
    if let Some(obj) = name_arg(args, "OBJECT_NAME") {
        filter.object = Some((name_arg(args, "OBJECT_LIBRARY"), obj, name_arg(args, "OBJECT_MEMBER")));
    }
    let objtype = args.word("OBJECT_OBJTYPE").filter(|t| !t.is_empty()).and_then(|t| types::type_by_name(&t)).map(|t| t.code);
    let user = args.word("USER").filter(|u| !u.is_empty() && u != "*ALL");
    let job = job_arg(args, "JOB");
    let program = args.word("PROGRAM").filter(|p| !p.is_empty() && p != "*ALL");
    let keep = |e: &jrn::Entry| {
        objtype.is_none_or(|t| e.object.as_ref().is_some_and(|o| o.type_code == t))
            && user.as_ref().is_none_or(|u| e.job.user == *u)
            && program.as_ref().is_none_or(|p| e.program == *p)
            && job.as_ref().is_none_or(|(n, u, j)| e.job.number == *n && (u.is_empty() || e.job.user == *u) && (j.is_empty() || e.job.name == *j))
    };
    let from = (args.word("STARTING_RECEIVER_LIBRARY").unwrap_or_default(), args.word("STARTING_RECEIVER_NAME").unwrap_or_default());
    let to = (args.word("ENDING_RECEIVER_LIBRARY").unwrap_or_default(), args.word("ENDING_RECEIVER_NAME").unwrap_or_default());
    for f in walk(txn, id, (&from.0, &from.1), (&to.0, &to.1), &filter, &keep)? {
        let e = &f.entry;
        out.row();
        common(out, &f);
        out.text("JOURNAL_CODE", char::from(e.code).to_string());
        out.text("JOURNAL_ENTRY_TYPE", String::from_utf8_lossy(&e.kind).into_owned());
        out.int("COUNT_OR_RRN", e.count);
        out.set("ENTRY_DATA", Value::Bin(e.data.clone()));
        if let Some(o) = &e.object {
            let member = o.member.map(|m| m.to_string()).unwrap_or_default();
            out.text("OBJECT", format!("{:<10}{:<10}{member}", o.object.to_string(), o.library.to_string()).trim_end().to_string());
            out.text_or_null("OBJECT_TYPE", types::type_by_code(o.type_code).map_or("", |t| t.name));
            out.text("OBJECT_TYPE_INDICATOR", if o.member.is_some() { "F" } else { "O" });
            out.int("OBJECT_ASP_NUMBER", 1);
            out.text("OBJECT_ASP_DEVICE", "*SYSBAS");
        }
        if e.commit_cycle != 0 {
            out.dec("COMMIT_CYCLE", &e.commit_cycle.to_string());
        }
        out.text("INDICATOR_FLAG", if e.flags & jrn::entry::flags::APPLIED != 0 { "1" } else { "0" });
    }
    Ok(())
}

/// Audit columns and the entry fields (by suffix) that fill them, the
/// first a layout has.
const AUDIT_FIELDS: &[(&str, &[&str])] = &[
    ("ENTRY_TYPE", &["ETYP"]),
    ("VIOLATION_TYPE", &["VIOL", "ETYP"]),
    ("OBJECT_LIBRARY", &["OLIB"]),
    ("OBJECT_NAME", &["ONAM"]),
    ("OBJECT_TYPE", &["OTYP"]),
    ("USER_PROFILE_NAME", &["USRP", "UPRF"]),
    ("AUDIT_USER_NAME", &["USRP"]),
    ("DEVICE_NAME", &["DEVN"]),
    ("COMMAND_NAME", &["CMDN"]),
    ("SYSTEM_VALUE", &["SYSV"]),
    ("NEW_VALUE", &["NVAL"]),
    ("OLD_VALUE", &["OVAL"]),
    ("PREVIOUS_OWNER", &["OOWN"]),
    ("NEW_OWNER", &["NOWN"]),
    ("OWNER", &["OWNR"]),
    ("AUTHORIZATION_LIST", &["AUTL"]),
    ("NEW_OBJECT_NAME", &["NNAM"]),
    ("NEW_OBJECT_LIBRARY", &["NLIB"]),
    ("SUBSYSTEM_NAME", &["SBSN"]),
    ("SUBSYSTEM_LIBRARY", &["SBSL"]),
    ("SPOOLED_FILE_NAME", &["FILE"]),
    ("OUTPUT_QUEUE", &["OUTQ"]),
    ("OUTPUT_QUEUE_LIBRARY", &["OQLB"]),
];

/// `SYSTOOLS.AUDIT_JOURNAL_xx`, entry type `A``B`.
pub fn audit<const A: u8, const B: u8>(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let code = [A, B];
    let Some(kind) = std::str::from_utf8(&code).ok().and_then(sec::audit::kind::Kind::by_code) else {
        return Ok(());
    };
    // Without QSYS/QAUDJRN nothing was audited.
    let Some(id) = sec::audit::write::journal(txn, ctx.qsys).map_err(|e| SqlError::Database { what: e.to_string(), msgid: None })? else {
        return Ok(());
    };
    let filter = jrn::Filter {
        codes: vec![jrn::code::AUDIT],
        types: vec![code],
        from_seq: seq(args, "STARTING_SEQUENCE"),
        to_seq: seq(args, "ENDING_SEQUENCE"),
        from_time: unix(args.get("STARTING_TIMESTAMP"))?,
        to_time: unix(args.get("ENDING_TIMESTAMP"))?,
        ..jrn::Filter::default()
    };
    let user = args.word("USER_NAME").filter(|u| !u.is_empty() && u != "*ALL");
    let job = job_arg(args, "JOB");
    let program = args.word("PROGRAM").filter(|p| !p.is_empty() && p != "*ALL");
    let keep = |e: &jrn::Entry| {
        user.as_ref().is_none_or(|u| e.job.user == *u)
            && program.as_ref().is_none_or(|p| e.program == *p)
            && job.as_ref().is_none_or(|(n, _, _)| e.job.number == *n)
    };
    let from = (args.word("STARTING_RECEIVER_LIBRARY").unwrap_or_default(), args.word("STARTING_RECEIVER_NAME").unwrap_or_default());
    let to = (args.word("ENDING_RECEIVER_LIBRARY").unwrap_or_default(), args.word("ENDING_RECEIVER_NAME").unwrap_or_default());
    for f in walk(txn, id, (&from.0, &from.1), (&to.0, &to.1), &filter, &keep)? {
        let Some(ev) = sec::audit::Event::decode(kind, &f.entry.data) else { continue };
        out.row();
        common(out, &f);
        for (col, suffixes) in AUDIT_FIELDS {
            if !out.has(col) {
                continue;
            }
            if let Some((_, v)) = ev.fields.iter().find(|(s, v)| suffixes.contains(s) && !v.trim().is_empty()) {
                out.text(col, v.trim());
            }
        }
        if !ev.path().is_empty() && out.has("PATH_NAME") {
            out.text("PATH_NAME", ev.path());
        }
    }
    Ok(())
}
