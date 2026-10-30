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
    "chgdbg",
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
    // The system's displays as panels, for a job that shows them.
    if let Some(res) = crate::panels::run(job, host, r, out) {
        return res;
    }
    // System Request, Attention, secondary and group jobs.
    if let Some(res) = crate::sysdsp::run(job, host, r, out) {
        return res;
    }
    match r.name.as_str() {
        "CALL" => {
            // The job servicing this one may have orders for the call.
            crate::devtools::dbg::served(job, host);
            return call(job, host, &parms);
        }
        "STRSEU" => return crate::devtools::seu::run(job, host, r),
        "RTVSYSVAL" => return rtvsysval(host, r, out),
        "RTVJOBA" => return rtvjoba(host, r, out),
        "RTVOBJD" => return rtvobjd(host, r, out),
        "RTVMBRD" => return rtvmbrd(host, r, out),
        "RTVUSRPRF" => return rtvusrprf(host, r, out),
        "RTVNETA" => return rtvneta(host, r, out),
        "RTVMSG" => return rtvmsg(host, r, out),
        "RTVCFGSTS" => return rtvcfgsts(host, r, out),
        "RTVCURDIR" => return rtvcurdir(host, r, out),
        "RCLRSC" => return Ok(rclrsc(job, host)),
        "STRTCPSVR" => return strtcpsvr(host, r),
        "STRTCP" | "ENDTCP" | "ENDTCPABN" | "WRKSRVTBLE" | "WRKPCLTBLE" | "WRKNETTBLE" | "CFGTCP"
        | "CFGTCPAPP" | "CFGTCPBP" | "CFGTCPRTD" | "CFGTCPSNMP" | "CFGTCPTELN" | "CFGTCPSMTP"
        | "CFGTCPPTP" => {
            if let Some(res) = super::tcp::run(job, host, r) {
                return res;
            }
        }
        "ENDTCPSVR" => return endtcpsvr(job, host, r),
        "CHGTELNA" => return chgtelna(job, host, r),
        // --- DDM and DRDA (command/ddm.rs)
        "ADDRDBDIRE" | "CHGRDBDIRE" | "RMVRDBDIRE" | "WRKRDBDIRE" | "CRTDDMF" | "CHGDDMF"
        | "CHGDDMTCPA" => {
            if let Some(res) = super::ddm::run(job, host, r) {
                return res;
            }
        }
        // --- STRDBG screens (devtools/dbg)
        "STRDBG" | "ENDDBG" | "DSPMODSRC" | "ADDBKP" | "RMVBKP" | "DSPBKP" | "STRSRVJOB"
        | "ENDSRVJOB" => {
            if let Some(res) = crate::devtools::dbg::run(job, host, r, out) {
                return res;
            }
        }
        // --- PDM (devtools/pdm)
        "STRPDM" | "WRKLIBPDM" | "WRKOBJPDM" | "WRKMBRPDM" | "FNDSTRPDM" => {
            return crate::devtools::pdm::run(job, host, r, out);
        }
        "CRTSCHIDX" | "ADDSCHIDXE" => return super::uiapi::command(host, r),
        // --- STRSST (devtools/sst)
        "STRSST" => return crate::devtools::sst::run(job, host, r, out),
        // --- DFU and RLU (devtools/dfu)
        "STRDFU" | "UPDDTA" | "DSPDTA" | "CHGDTA" => {
            return crate::devtools::dfu::run(job, host, r, out);
        }
        "STRRLU" => return crate::devtools::rlu::run(job, host, r, out),
        // --- STRSQL (devtools/isql)
        "STRSQL" => return crate::devtools::isql::run(host, r),
        // --- SDA (devtools/sda)
        "STRSDA" => return crate::devtools::sda::run(job, host, r, out),
        // --- EDTF and DSPF (devtools/edtf); DSPF without a display is the service's text
        "EDTF" | "DSPF" => {
            if let Some(res) = crate::devtools::edtf::run(job, host, r, out) {
                return res;
            }
        }
        _ => {}
    }
    // A panel's command in a job that shows none: the service's text.
    if sys_cpps::inventory::in_service_too(&r.name) {
        return super::service(job, host, r, out);
    }
    let line = sys_cpps::bridge::specified_line(&entry.word, &parms);
    let out = jobcmd::command(job, host, &entry.word, &line);
    // A display's first line may well start with a message identifier
    // (the job log's, the queue's): no escape in that.
    if matches!(entry.word.as_str(), "dspjoblog" | "dspmsgq" | "dmp") {
        return Ok(out);
    }
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

/// `RTVOBJD OBJ(LIB/NAME) OBJTYPE(*TYPE)`: the object's description
/// through the job's link.
fn rtvobjd(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let (name, lib) = match r.get("OBJ").and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(Value::source).unwrap_or_default(),
            parts.get(1).map_or_else(|| "*LIBL".into(), Value::source),
        ),
        Some(v) => (v.source(), "*LIBL".into()),
        None => return Err(Fail::new("CPD0015", "OBJ is required")),
    };
    let ty = r
        .get("OBJTYPE")
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default();
    let found = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), ty.clone()));
    let Some(o) = found else {
        return Err(Fail {
            msgid: "CPF9801".into(),
            data: cmd::error::data(&[(&name, 10), (&lib, 10)]),
            text: format!("CPF9801 {ty} {lib}/{name} was not found."),
        });
    };
    let text = |s: &str| Value::Char(s.to_string());
    let values = [
        ("RTNLIB", text(&o.lib)),
        ("OBJATR", text(&o.attr)),
        ("TEXT", text(&o.text)),
        ("OWNER", text(&o.owner)),
        ("CRTDATE", text(&o.created)),
        ("CHGDATE", text(&o.changed)),
        ("CRTUSER", text(&o.owner)),
        ("USRATR", text(&o.user_attr)),
        ("OBJDMN", text("*U")),
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
    let ext_before = job.ext_msgq.msgs.len();
    let r = job.call_program(&name, &lib, &addrs);
    let lines: Vec<String> = job.console.drain(before..).collect();
    let mut out = String::new();
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    // What the program sent to the job's external queue: shown as a
    // user at Command Entry would see it.
    for m in job.ext_msgq.msgs.iter().skip(ext_before) {
        let text = m.text.trim_end();
        // Inquiries were answered on their own queue; escapes end the call.
        let shown = !matches!(
            m.msg_type,
            qir::MsgType::Escape | qir::MsgType::Inq | qir::MsgType::Rpy
        );
        if !text.is_empty() && shown {
            out.push_str(text);
            out.push('\n');
        }
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

/// A parameter's text, empty when absent.
fn text_of(r: &Resolved, kw: &str) -> String {
    r.get(kw)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
}

/// A qualified parameter as `(name, library)`, the library `*LIBL`
/// when not given.
fn qual_of(r: &Resolved, kw: &str) -> (String, String) {
    match r.get(kw).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(Value::source).unwrap_or_default(),
            parts.get(1).map_or_else(|| "*LIBL".into(), Value::source),
        ),
        Some(v) => (v.source(), "*LIBL".into()),
        None => (String::new(), "*LIBL".into()),
    }
}

fn num(n: i64) -> Value {
    Value::Num(qdecimal::Decimal::new(i128::from(n), 0).expect("small"))
}

fn chars(s: &str) -> Value {
    Value::Char(s.to_string())
}

/// `CYYMMDD` of a Unix time; `*NONE` for zero.
fn cyymmdd(secs: u64) -> String {
    if secs == 0 {
        return "*NONE".into();
    }
    let civil = sec::cmd::civil(secs);
    let (y, rest) = civil.split_once('-').unwrap_or(("1970", "01-01"));
    let year: u32 = y.parse().unwrap_or(1970);
    format!(
        "{}{:02}{}",
        year.saturating_sub(1900) / 100,
        year % 100,
        rest.replace('-', "")
    )
}

/// `RTVMBRD FILE(LIB/FILE) MBR(name)`: the member's description.
fn rtvmbrd(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let (name, lib) = qual_of(r, "FILE");
    let want = text_of(r, "MBR").to_ascii_uppercase();
    let info = host
        .link
        .borrow_mut()
        .with(|l| l.file(lib.clone(), name.clone()));
    let members = host
        .link
        .borrow_mut()
        .with(|l| l.members(lib.clone(), name.clone()));
    let Some(f) = info else {
        return Err(Fail {
            msgid: "CPF9812".into(),
            data: cmd::error::data(&[(&name, 10), (&lib, 10)]),
            text: format!("CPF9812 file {name} in library {lib} not found."),
        });
    };
    let m = match want.as_str() {
        "" | "*FIRST" => members.first(),
        "*LAST" => members.last(),
        n => members.iter().find(|m| m.name == n),
    };
    let Some(m) = m else {
        return Err(Fail {
            msgid: "CPF9815".into(),
            data: cmd::error::data(&[(&want, 10), (&name, 10), (&lib, 10)]),
            text: format!("CPF9815 member {want} of file {name} in library {lib} not found."),
        });
    };
    let values = [
        ("RTNLIB", chars(&f.lib)),
        ("RTNSYSTEM", chars("*LCL")),
        ("RTNMBR", chars(&m.name)),
        ("FILEATR", chars(&f.attr)),
        ("FILETYPE", chars(if f.source { "*SRC" } else { "*DATA" })),
        (
            "SRCTYPE",
            chars(if m.srctype.is_empty() {
                "*NONE"
            } else {
                &m.srctype
            }),
        ),
        ("CRTDATE", chars(&m.created)),
        ("EXPDATE", chars("*NONE")),
        ("TEXT", chars(&m.text)),
        ("NBRCURRCD", num(i64::from(m.records))),
        ("NBRDLTRCD", num(i64::from(m.deleted))),
        (
            "DTASPCSIZ",
            num(i64::from(m.records + m.deleted) * i64::from(f.rec_len)),
        ),
        ("ACCPTHSIZ", num(0)),
        ("NBRDTAMBRS", num(i64::from(f.attr == "LF"))),
        ("CHGDATE", chars(&m.changed)),
        ("SAVDATE", chars("")),
        ("RSTDATE", chars("")),
        ("NBRDAYUSED", num(0)),
        ("USEDATE", chars("")),
        ("SRCCHGDATE", chars(&m.changed)),
        ("JOINMBR", chars("*NO")),
    ];
    returned(r, &values, out)
}

/// `RTVUSRPRF USRPRF(name)`: the profile's attributes.
#[allow(clippy::too_many_lines)] // one entry per return parameter
fn rtvusrprf(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let mut user = text_of(r, "USRPRF").to_ascii_uppercase();
    if user.is_empty() || user == "*CURRENT" {
        user.clone_from(&host.start.user);
    }
    let answer = host
        .link
        .borrow_mut()
        .with(|l| l.sec_api(sec::api::Query::Profile { user: user.clone() }));
    let p = match answer {
        Ok(sec::api::Answer::Profile { data }) => {
            sec::profile::UsrPrf::decode(&data).map_err(|e| Fail::new("CPF9999", e.to_string()))?
        }
        Ok(_) => return Err(Fail::new("CPF9999", "unexpected answer".to_string())),
        Err(e) => {
            return Err(Fail {
                msgid: e.msgid.clone(),
                data: cmd::error::data(&[(&user, 10)]),
                text: format!("{} user profile {user} not found.", e.msgid),
            });
        }
    };
    let qual = |t: &(String, String)| (chars(&t.0), chars(&t.1));
    let (inlpgm, inlpgmlib) = qual(&p.inlpgm);
    let (jobd, jobdlib) = qual(&p.jobd);
    let (msgq, msgqlib) = qual(&p.msgq);
    let (outq, outqlib) = qual(&p.outq);
    let (inlmnu, inlmnulib) = qual(&p.inlmnu);
    let (atnpgm, atnpgmlib) = qual(&p.atnpgm);
    let (srtseq, srtseqlib) = qual(&p.srtseq);
    let padded = |v: &[String]| {
        let mut t = String::new();
        for n in v {
            let _ = write!(t, "{n:<10}");
        }
        t.trim_end().to_string()
    };
    let spcaut: Vec<String> = p.spcaut.names().iter().map(|s| (*s).to_string()).collect();
    let yes_no = |b: bool| chars(if b { "*YES" } else { "*NO" });
    let values = [
        ("RTNUSRPRF", chars(&p.name)),
        ("SPCAUT", chars(&padded(&spcaut))),
        ("MAXSTG", num(p.maxstg)),
        ("MAXSTGLRG", num(p.maxstglrg)),
        (
            "STGUSED",
            num(i64::try_from(p.stg_used).unwrap_or(i64::MAX)),
        ),
        (
            "STGUSEDLRG",
            num(i64::try_from(p.stg_used).unwrap_or(i64::MAX)),
        ),
        ("PTYLMT", chars(&p.ptylmt.to_string())),
        ("INLPGM", inlpgm),
        ("INLPGMLIB", inlpgmlib),
        ("JOBD", jobd),
        ("JOBDLIB", jobdlib),
        (
            "GRPPRF",
            chars(if p.grpprf.is_empty() {
                "*NONE"
            } else {
                &p.grpprf
            }),
        ),
        (
            "OWNER",
            chars(if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" }),
        ),
        ("GRPAUT", chars(&p.grpaut)),
        (
            "GRPAUTTYP",
            chars(if p.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" }),
        ),
        ("ACGCDE", chars(&p.acgcde)),
        ("MSGQ", msgq),
        ("MSGQLIB", msgqlib),
        ("DLVRY", chars(&p.dlvry)),
        ("SEV", num(i64::from(p.sev))),
        ("OUTQ", outq),
        ("OUTQLIB", outqlib),
        ("PRTDEV", chars(&p.prtdev)),
        ("TEXT", chars(&p.text)),
        ("SPCENV", chars(&p.spcenv)),
        ("DSPSGNINF", chars(p.dspsgninf.name())),
        ("PWDEXPITV", num(i64::from(p.pwdexpitv))),
        ("PWDEXP", yes_no(p.pwdexp)),
        ("USRCLS", chars(p.usrcls.name())),
        (
            "STATUS",
            chars(if p.disabled { "*DISABLED" } else { "*ENABLED" }),
        ),
        ("PWDCHGDAT", chars(&cyymmdd(p.pwd_changed))),
        ("PRVSIGN", chars(&cyymmdd(p.prev_signon))),
        ("NOTVLDSIGN", num(i64::from(p.signon_invalid))),
        ("USROPT", chars(&padded(&p.usropt))),
        (
            "CURLIB",
            chars(if p.curlib.is_empty() {
                "*CRTDFT"
            } else {
                &p.curlib
            }),
        ),
        ("INLMNU", inlmnu),
        ("INLMNULIB", inlmnulib),
        ("LMTCPB", chars(p.lmtcpb.name())),
        ("LMTDEVSSN", chars(p.lmtdevssn.name())),
        ("KBDBUF", chars(&p.kbdbuf)),
        ("ASTLVL", chars(&p.astlvl)),
        ("ATNPGM", atnpgm),
        ("ATNPGMLIB", atnpgmlib),
        ("SRTSEQ", srtseq),
        ("SRTSEQLIB", srtseqlib),
        ("LANGID", chars(&p.langid)),
        ("CNTRYID", chars(&p.cntryid)),
        ("CCSID", num(i64::from(p.ccsid))),
        ("CHRIDCTL", chars(&p.chridctl)),
        ("SETJOBATR", chars(&padded(&p.setjobatr))),
        ("LOCALE", chars(&p.locale)),
        ("SUPGRPPRF", chars(&padded(&p.supgrpprf))),
        ("OBJAUD", chars(&p.objaud)),
        ("AUDLVL", chars(&padded(&p.audlvl))),
        ("UID", num(i64::from(p.uid))),
        ("GID", num(i64::from(p.gid))),
        ("HOMEDIR", chars(&p.homedir)),
        ("LCLPWDMGT", yes_no(p.lclpwdmgt)),
        (
            "PWDCHGBLK",
            chars(&if p.pwdchgblk == 0 {
                "*NONE".to_string()
            } else {
                p.pwdchgblk.to_string()
            }),
        ),
        ("USREXPDATE", chars(&cyymmdd(p.usrexpdate))),
        ("USREXPITV", num(i64::from(p.usrexpitv))),
    ];
    returned(r, &values, out)
}

/// The network attributes `RTVNETA` returns, in its keyword order.
const NETA_KEYWORDS: &[&str] = &[
    "SYSNAME",
    "PNDSYSNAME",
    "LCLNETID",
    "LCLCPNAME",
    "LCLLOCNAME",
    "DFTMODE",
    "NODETYPE",
    "DTACPR",
    "DTACPRINM",
    "MAXINTSSN",
    "RAR",
    "NETSERVER",
    "ALRSTS",
    "ALRPRIFP",
    "ALRDFTFP",
    "ALRLOGSTS",
    "ALRBCKFP",
    "ALRRQSFP",
    "ALRCTLD",
    "ALRHLDCNT",
    "ALRFTR",
    "MSGQ",
    "OUTQ",
    "JOBACN",
    "MAXHOP",
    "DDMACC",
    "PCSACC",
    "DFTCNNLST",
    "ALWANYNET",
    "NWSDOMAIN",
    "ALWVRTIP",
    "ALWHPRTWR",
    "VRTAUTODEV",
    "HPRPTHTMR",
    "ALWADDCLU",
    "MDMCNTRYID",
];

/// `RTVNETA`: every attribute asked for, through the job's link.
fn rtvneta(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let mut values: Vec<(&str, Value)> = Vec::new();
    for k in NETA_KEYWORDS {
        let Some(v) = host.link.borrow_mut().with(|l| l.netattr((*k).to_string())) else {
            continue;
        };
        match v.kind {
            sysapi::SysValKind::Bin => {
                let n = v
                    .data
                    .get(..4)
                    .and_then(|b| b.try_into().ok())
                    .map_or(0, i32::from_be_bytes);
                values.push((k, num(i64::from(n))));
            }
            sysapi::SysValKind::Char => {
                let text = ccsid::from_ebcdic(&v.data).trim_end().to_string();
                match *k {
                    "MSGQ" | "OUTQ" | "ALRFTR" | "DDMACC" | "PCSACC" => {
                        let (lib, name) = text.split_once('/').unwrap_or(("", text.as_str()));
                        let lib_kw: &'static str = match *k {
                            "MSGQ" => "MSGQLIB",
                            "OUTQ" => "OUTQLIB",
                            "ALRFTR" => "ALRFTRLIB",
                            "DDMACC" => "DDMACCLIB",
                            _ => "PCSACCLIB",
                        };
                        values.push((k, chars(name)));
                        values.push((lib_kw, chars(lib)));
                    }
                    _ => values.push((k, chars(&text))),
                }
            }
        }
    }
    returned(r, &values, out)
}

/// `RTVMSG MSGID(id) MSGF(lib/file) MSGDTA(data)`: the message text.
fn rtvmsg(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let msgid = text_of(r, "MSGID").to_ascii_uppercase();
    let (msgf, lib) = qual_of(r, "MSGF");
    let data = match r.get("MSGDTA").and_then(|b| b.value.as_ref()) {
        Some(Value::Hex(b)) => b.clone(),
        Some(v) => ccsid::to_ebcdic(&v.source()).unwrap_or_default(),
        None => Vec::new(),
    };
    let found = host
        .link
        .borrow_mut()
        .with(|l| l.describe(msgid.clone(), msgf.clone(), lib.clone()));
    let Some(d) = found else {
        return Err(Fail {
            msgid: "CPF2419".into(),
            data: cmd::error::data(&[(&msgid, 7), (&msgf, 10), (&lib, 10)]),
            text: format!("CPF2419 message {msgid} not found in message file {lib}/{msgf}."),
        });
    };
    let msg = d.format(&data);
    let help = d.format_help(&data);
    let values = [
        ("MSG", chars(&msg)),
        ("MSGLEN", num(i64::try_from(msg.len()).unwrap_or(0))),
        ("SECLVL", chars(&help)),
        ("SECLVLLEN", num(i64::try_from(help.len()).unwrap_or(0))),
        ("SEV", num(i64::from(d.severity))),
        ("ALROPT", chars("*NO")),
        ("LOGPRB", chars("*NO")),
        ("CCSID", num(37)),
        ("TXTCCSID", num(37)),
    ];
    returned(r, &values, out)
}

/// `STRTCPSVR SERVER(*TELNET)`: the telnet server job `QTVTELNET`
/// starts in `QSYSWRK`, a child of this job like a subsystem monitor.
/// The other servers wait for Phase 10.
fn strtcpsvr(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let servers: Vec<String> = r
        .parms
        .iter()
        .find(|b| b.kwd == "SERVER")
        .and_then(|b| b.value.as_ref())
        .map_or_else(
            || vec!["*ALL".to_string()],
            |v| {
                v.source()
                    .split_whitespace()
                    .map(|s| {
                        s.trim_matches(|c| c == '(' || c == ')')
                            .to_ascii_uppercase()
                    })
                    .collect()
            },
        );
    let telnet = servers.iter().any(|s| s == "*TELNET" || s == "*ALL");
    if !telnet {
        return Err(Fail::new(
            "TCP1A04",
            format!(
                "{}: only *TELNET starts in this release.",
                servers.join(" ")
            ),
        ));
    }
    let state = host
        .link
        .borrow_mut()
        .with(|l| l.job_state("QTVTELNET".into()));
    if state == 1 {
        return Ok("TCP1A08 TELNET server already started.\n".into());
    }
    let number = host
        .link
        .borrow_mut()
        .with(Client::job_number)
        .map_err(|e| Fail::new("CPF0001", e))?;
    let start = wm::ctl::Start {
        name: "QTVTELNET".into(),
        user: "QTCP".into(),
        number,
        handle: u64::from(number),
        kind: 'B',
        subsystem: "QSYSWRK".into(),
        role: "TELNET".into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        pool: "*BASE".into(),
        rqsdta: telnet_port(host),
        ..wm::ctl::Start::default()
    };
    let mut s = crate::work::spawn::job(start).map_err(|e| Fail::new("TCP1A05", e))?;
    crate::work::spawn::ready(&mut s).map_err(|e| Fail::new("TCP1A05", e))?;
    host.children.borrow_mut().push(s);
    Ok("TCP1A07 TELNET server starting.\n".into())
}

/// `PORT(n)` for the server from `CHGTELNA`'s data area, when one was
/// set (the harness uses a high port where 23 is not allowed).
fn telnet_port(host: &Host) -> String {
    host.link
        .borrow_mut()
        .with(|l| l.da_read("QTVTELNA".into(), "QUSRSYS".into()))
        .ok()
        .map(|v| ccsid::from_ebcdic(&v.value))
        .and_then(|t| {
            t.to_ascii_uppercase()
                .split_once("PORT(")
                .map(|(_, r)| format!("PORT({})", r.split(')').next().unwrap_or("23").trim()))
        })
        .unwrap_or_default()
}

/// `ENDTCPSVR SERVER(*TELNET)`: the server job ends.
fn endtcpsvr(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let _ = r;
    let state = host
        .link
        .borrow_mut()
        .with(|l| l.job_state("QTVTELNET".into()));
    if state != 1 {
        return Ok("TCP1A09 TELNET server not active.\n".into());
    }
    let out = crate::command::run(job, host, "ENDJOB JOB(QTVTELNET) OPTION(*CNTRLD)").text;
    if let Some(id) = leading_msgid(out.trim()) {
        return Err(Fail::new(id, out.trim().to_string()));
    }
    Ok("TCP1A0A TELNET server ending.\n".into())
}

/// `CHGTELNA`: the attributes kept in the data area `QUSRSYS/QTVTELNA`
/// as `KEYWORD(value)` text.
fn chgtelna(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let current = host
        .link
        .borrow_mut()
        .with(|l| l.da_read("QTVTELNA".into(), "QUSRSYS".into()))
        .ok()
        .map(|v| ccsid::from_ebcdic(&v.value).trim().to_string())
        .unwrap_or_default();
    let mut values: Vec<(String, String)> = current
        .split(')')
        .filter_map(|p| p.trim().split_once('('))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    for b in &r.parms {
        let Some(v) = b.value.as_ref() else {
            continue;
        };
        let text = v.source().trim().to_ascii_uppercase();
        if text == "*SAME" || text.is_empty() {
            continue;
        }
        values.retain(|(k, _)| k != &b.kwd);
        values.push((b.kwd.clone(), text));
    }
    let joined: Vec<String> = values.iter().map(|(k, v)| format!("{k}({v})")).collect();
    let text = joined.join(" ");
    let out = crate::command::run(
        job,
        host,
        &format!(
            "CHGDTAARA DTAARA(QUSRSYS/QTVTELNA (1 200)) VALUE('{}')",
            text.replace('\'', "''")
        ),
    )
    .text;
    if leading_msgid(out.trim()).is_some_and(|id| id.starts_with("CPF")) {
        let made = crate::command::run(
            job,
            host,
            "CRTDTAARA DTAARA(QUSRSYS/QTVTELNA) TYPE(*CHAR) LEN(200) TEXT('TELNET server attributes')",
        )
        .text;
        if let Some(id) = leading_msgid(made.trim()) {
            return Err(Fail::new(id, made.trim().to_string()));
        }
        let again = crate::command::run(
            job,
            host,
            &format!(
                "CHGDTAARA DTAARA(QUSRSYS/QTVTELNA (1 200)) VALUE('{}')",
                text.replace('\'', "''")
            ),
        )
        .text;
        if let Some(id) = leading_msgid(again.trim()) {
            return Err(Fail::new(id, again.trim().to_string()));
        }
    }
    Ok("TELNET attributes changed.\n".into())
}

/// `RTVCFGSTS CFGD(name) CFGTYPE(*LIN|*CTL|*DEV)`: the status code
/// (0 varied off, 30 varied on, 60 active, 80 recovery pending, ...)
/// and its text, from the storage service.
fn rtvcfgsts(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let name = text_of(r, "CFGD").to_ascii_uppercase();
    let ty = text_of(r, "CFGTYPE").to_ascii_uppercase();
    if !matches!(ty.as_str(), "*LIN" | "*CTL" | "*DEV") {
        return Err(Fail::new(
            "CPD0084",
            format!("CFGTYPE({ty}): lines, controllers and devices have a status here"),
        ));
    }
    let found = host
        .link
        .borrow_mut()
        .with(|l| l.cfg_status(name.clone(), ty.clone()));
    let Some((code, text)) = found else {
        let msgid = match ty.as_str() {
            "*LIN" => cmncfg::msgs::LINE_NOT_FOUND,
            "*CTL" => cmncfg::msgs::CTL_NOT_FOUND,
            _ => cmncfg::msgs::DEV_NOT_FOUND,
        };
        return Err(Fail {
            msgid: msgid.into(),
            data: cmd::error::data(&[(&name, 10)]),
            text: format!("{msgid} {} {name} not found.", ty.trim_start_matches('*').to_ascii_lowercase()),
        });
    };
    let values = [("STSCDE", num(i64::from(code))), ("STS", chars(&text))];
    returned(r, &values, out)
}

/// `RCLRSC`: the files left open by default activation group programs
/// no longer on the stack are closed, and the file overrides of call
/// levels that have returned go.
fn rclrsc(job: &mut Job, host: &Host) -> String {
    let closed = job.rclrsc();
    if job.ovr_top > job.depth {
        let _ = host.exec(&format!("dltovr ENDLVL({})", job.depth + 1));
        job.ovr_top = job.depth;
    }
    if closed > 0 {
        format!("{closed} file(s) closed.\n")
    } else {
        String::new()
    }
}

/// `RTVCURDIR RTNDIR(&dir) DIRNAMLEN(&len)`: the job's current
/// directory, kept by the storage service for the job's connection.
fn rtvcurdir(host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    use ifs::client::IfsClient as _;
    let c = stg::remote::RemoteIfs::new(host.link.clone());
    let dir = c.getcwd().map_err(|e| {
        let m = ifscmd::Msg::errno(e, ".");
        Fail {
            msgid: m.id,
            data: m.data,
            text: e.text().to_string(),
        }
    })?;
    let len = i128::try_from(dir.len()).unwrap_or(0);
    let values = [
        ("RTNDIR", Value::Char(dir)),
        (
            "DIRNAMLEN",
            Value::Num(qdecimal::Decimal::new(len, 0).expect("small")),
        ),
    ];
    returned(r, &values, out)
}
