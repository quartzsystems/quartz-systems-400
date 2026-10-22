// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The processing programs that run in the job itself: `CALL`, the
//! activation groups, the call stack, the debugger, the job's own
//! message queues, `STRSBS` (the job spawns the monitor), `SBMJOB`,
//! `DLYJOB`, `PWRDWNSYS`, `DSCJOB`, `CHGPWD`. Most unpack the
//! parameter block into the keyword form the Phase 5 functions take.

use std::fmt::Write as _;

use mi::Job;
use stg::Client;

use cmd::block;
use cmd::{Resolved, Value};

use super::{Fail, Outcome, parms_of};
use crate::console::host::Host;
use crate::console::jobcmd;

/// The old console words the job answers itself, for `svc.` lines.
pub const OLD_JOB_WORDS: &[&str] = &[
    "dspactgrp",
    "rclactgrp",
    "dspcallstk",
    "dspmsgq",
    "rpy",
    "dmp",
    "strdbg",
    "enddbg",
    "addbkp",
    "rmvbkp",
    "dspbkp",
    "dspjoblog",
    "strsbs",
    "dlyjob",
    "sbmjob",
    "pwrdwnsys",
    "wrkjob",
    "chgpwd",
    "dscjob",
    "endsys",
];

/// Run a job-site command.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let parms = parms_of(r);
    let entry = sys_cpps::entry(&r.name).expect("a job-site command is in the inventory");
    match r.name.as_str() {
        "CALL" => return call(job, host, &parms),
        "RTVSYSVAL" => return rtvsysval(host, r, out),
        "RTVJOBA" => return rtvjoba(host, r, out),
        _ => {}
    }
    let line = sys_cpps::bridge::specified_line(&entry.word, &parms);
    let out = jobcmd::command(job, host, &entry.word, &line);
    classify(out)
}

/// Output whose first line starts with a message identifier is the
/// escape that ended the command.
pub fn classify(out: String) -> Result<String, Fail> {
    let first = out.lines().next().unwrap_or("").trim();
    if let Some(id) = leading_msgid(first) {
        return Err(Fail {
            msgid: id.to_string(),
            data: Vec::new(),
            text: out.trim_end().to_string(),
        });
    }
    Ok(out)
}

/// `AAAnnnn` at the start of a line, followed by a blank or the end.
#[must_use]
pub fn leading_msgid(line: &str) -> Option<&str> {
    let b = line.as_bytes();
    if b.len() < 7 {
        return None;
    }
    let id = &line[..7];
    let ok = b[..3].iter().all(u8::is_ascii_uppercase)
        && b[3..7].iter().all(u8::is_ascii_hexdigit)
        && b.get(7).is_none_or(|c| *c == b' ' || *c == b':');
    ok.then_some(id)
}

/// Return values into the block: the parameters named take the
/// values, and the block goes back to the caller as `segments`.
fn returned(r: &Resolved, values: &[(&str, Value)], out: &mut Outcome) -> Result<String, Fail> {
    let mut parms = r.parms.clone();
    for (kwd, v) in values {
        if let Some(b) = parms.iter_mut().find(|b| b.kwd == *kwd) {
            b.value = Some(v.clone());
        }
    }
    let built = block::build(&r.def, &parms).map_err(|e| Fail::new(e.msgid(), e.to_string()))?;
    out.segments = Some(built.segments);
    Ok(String::new())
}

/// `RTVSYSVAL SYSVAL(name) RTNVAR(&var)`: the value as text, a number
/// in digits.
fn rtvsysval(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let name = r
        .get("SYSVAL")
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default();
    let Some(v) = host.link.borrow_mut().with(|l| l.sysval(name.clone())) else {
        return Err(Fail::new(
            "CPD0084",
            format!("SYSVAL: {name} is not a system value"),
        ));
    };
    let text = match v.kind {
        sysapi::SysValKind::Char => ccsid::from_ebcdic(&v.data),
        sysapi::SysValKind::Bin => {
            let n = v
                .data
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map_or(0, i32::from_be_bytes);
            n.to_string()
        }
    };
    returned(r, &[("RTNVAR", Value::Char(text))], out)
}

/// `RTVJOBA`: the job's names and attributes into the variables named.
fn rtvjoba(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let a = host.attrs();
    let s = &host.start;
    let text = |s: &str| Value::Char(s.to_string());
    let num = |n: i64| Value::Num(qdecimal::Decimal::new(i128::from(n), 0).expect("small"));
    let names = |v: &[String]| {
        let mut t = String::new();
        for n in v {
            let _ = write!(t, "{n:<10}");
        }
        t.trim_end().to_string()
    };
    let (outq, outqlib) = a
        .outq
        .split_once('/')
        .map_or((a.outq.as_str(), ""), |(l, n)| (n, l));
    let (jobd, jobdlib) = a
        .jobd
        .split_once('/')
        .map_or((a.jobd.as_str(), ""), |(l, n)| (n, l));
    let values = [
        ("JOB", text(&s.name)),
        ("USER", text(&s.user)),
        ("NBR", text(&format!("{:06}", s.number))),
        ("CURUSER", text(&s.user)),
        ("TYPE", text(&s.kind.to_string())),
        ("SBS", text(&s.subsystem)),
        ("SWS", text(&a.sws)),
        ("CURLIB", text(a.libl.cur.as_deref().unwrap_or("*NONE"))),
        ("SYSLIBL", text(&names(&a.libl.sys))),
        ("USRLIBL", text(&names(&a.libl.usr))),
        ("LOGLVL", num(i64::from(a.log_level))),
        ("LOGSEV", num(i64::from(a.log_sev))),
        ("LOGCLPGM", text(&a.logclpgm)),
        ("RUNPTY", num(i64::from(a.runpty))),
        ("OUTQ", text(outq)),
        ("OUTQLIB", text(outqlib)),
        ("JOBD", text(jobd)),
        ("JOBDLIB", text(jobdlib)),
        ("DATFMT", text(&a.datfmt)),
        ("DATSEP", text(&a.datsep)),
        ("TIMSEP", text(&a.timsep)),
        ("DECFMT", text(&a.decfmt)),
        ("INQMSGRPY", text(&a.inqmsgrpy)),
        ("LANGID", text(&a.langid)),
        ("CNTRYID", text(&a.cntryid)),
        ("CCSID", num(i64::from(a.ccsid))),
        ("DFTCCSID", num(i64::from(a.dftccsid))),
    ];
    returned(r, &values, out)
}

/// `CALL PGM(LIB/NAME) PARM(values)`: character values as typed,
/// numbers as `packed(15,5)`, hex as bytes.
fn call(job: &mut Job, _host: &Host, parms: &cmd::Parms) -> Result<String, Fail> {
    let (name, lib) = parms.qual("PGM");
    let lib = if lib.is_empty() {
        "*LIBL".to_string()
    } else {
        lib
    };
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for v in parms.list("PARM") {
        buffers.push(match v {
            cmd::Value::Num(d) => {
                let p = qdecimal::Packed::new(15, 5).expect("valid");
                p.encode(d, qdecimal::Rounding::Truncate)
                    .map_err(|e| Fail::new("CPD0084", e.to_string()))?
            }
            cmd::Value::Hex(b) => b.clone(),
            cmd::Value::Char(s) => {
                ccsid::to_ebcdic(s).map_err(|_| Fail::new("CPD0084", format!("{s}: not CP037")))?
            }
            other => ccsid::to_ebcdic(&other.source()).unwrap_or_default(),
        });
    }
    let addrs: Vec<u64> = buffers.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    let before = job.console.len();
    let r = job.call_program(&name, &lib, &addrs);
    let lines: Vec<String> = job.console.drain(before..).collect();
    let mut out = String::new();
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    if let Err(e) = r {
        return Err(Fail {
            msgid: e.msgid().clone(),
            data: Vec::new(),
            text: format!("{out}{}: {lib}/{name} could not be called.", e.msgid()),
        });
    }
    if let Some(e) = job.clear() {
        if e.msgid == "CEE9903" {
            out.push_str("Program ended by CEETREC.\n");
        } else {
            return Err(Fail {
                msgid: e.msgid.clone(),
                data: e.data.clone(),
                text: format!("{out}Exception {} left for the caller.", e.msgid),
            });
        }
    }
    for (i, b) in buffers.iter().enumerate() {
        if b.iter().all(u8::is_ascii_graphic) {
            continue;
        }
        let _ = writeln!(out, "Parm {}: {}", i + 1, jobcmd::parm_text(b));
    }
    Ok(out)
}
