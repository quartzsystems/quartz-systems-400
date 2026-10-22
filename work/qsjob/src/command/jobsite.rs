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

use super::{Fail, parms_of};
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
pub fn run(job: &mut Job, host: &Host, r: &cmd::Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let entry = sys_cpps::entry(&r.name).expect("a job-site command is in the inventory");
    if r.name == "CALL" {
        return call(job, host, &parms);
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
