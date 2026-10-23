// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKJOB`, `DSPJOB` and `DSPJOBLOG` (`QUIWRKJOB`): the job menus and
//! the displays behind their options. The job's own log, call stack
//! and open files come from the job itself; another job's from the
//! storage service where it keeps them.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::proto::JobDetail;

use super::{
    Flow, common, date_time, entry, parm, refill, render_fail, run_options, show_display,
};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

const GROUP: &str = "QUIWRKJOB";

/// How a display under the menu was left.
pub enum Left {
    /// F3: leave the command.
    Exit,
    /// Enter, F12 or F16: back to the menu.
    Back,
}

/// `WRKJOB [JOB()] [OPTION()]`.
pub fn wrkjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    menu(job, host, surface, r, true)
}

/// `DSPJOB [JOB()] [OPTION()]`.
pub fn dspjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    menu(job, host, surface, r, false)
}

/// `DSPJOBLOG [JOB()]`.
pub fn dspjoblog(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let detail = fetch(host, &target(r))?;
    let mut d = crate::panels::dialog(host, GROUP)?;
    identify(&mut d, &detail);
    d.set("CANCHG", "N");
    joblog(job, host, surface, &mut d, &detail)?;
    Ok(String::new())
}

/// The job named by `JOB`, blank for the job itself.
fn target(r: &Resolved) -> String {
    parm(r, "JOB").filter(|j| j != "*").unwrap_or_default()
}

/// A job's detail, or the escape saying it is not there.
fn fetch(host: &Host, job: &str) -> Result<JobDetail, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.job_detail(job.to_string()))
        .map_err(|e| {
            let (id, text) = e.split_once(' ').unwrap_or(("CPF1070", e.as_str()));
            Fail::new(id, text)
        })
}

/// The job's names on every panel of the group.
fn identify(d: &mut UimDialog, detail: &JobDetail) {
    let j = &detail.job;
    d.set("JOB", &j.name);
    d.set("USER", &j.user);
    d.set("NUMBER", &format!("{:06}", j.number));
    d.set(
        "JOBHDR",
        &format!(
            "Job . . :   {:<10}    User . . :   {:<10}    Number . . . :   {:06}",
            j.name, j.user, j.number
        ),
    );
}

/// Whether the job is the one running this display.
fn own(host: &Host, detail: &JobDetail) -> bool {
    detail.job.number == host.start.number
}

fn menu(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, work: bool) -> Result<String, Fail> {
    let q = target(r);
    let mut detail = fetch(host, &q)?;
    let mut d = crate::panels::dialog(host, GROUP)?;
    identify(&mut d, &detail);
    d.set("CANCHG", if work { "Y" } else { "N" });
    let direct = match parm(r, "OPTION").as_deref() {
        None | Some("*SELECT") => None,
        Some("*STSA") => Some(1),
        Some("*DFNA") => Some(2),
        Some("*RUNA") => Some(3),
        Some("*SPLF") => Some(4),
        Some("*JOBLOG") => Some(10),
        Some("*PGMSTK") => Some(11),
        Some("*JOBLCK") => Some(12),
        Some("*LIBL") => Some(13),
        Some("*OPNF") => Some(14),
        Some("*FILOVR") => Some(15),
        Some("*CMTCTL") => Some(16),
        Some("*CMNSTS") => Some(17),
        Some("*ACTGRP") => Some(18),
        Some("*MUTEX") => Some(19),
        Some("*THREAD") => Some(20),
        Some(o) => return Err(Fail::new("CPD0084", format!("OPTION({o}) is not valid"))),
    };
    if let Some(n) = direct {
        option(job, host, surface, &mut d, &detail, n)?;
        return Ok(String::new());
    }
    let panel = if work { "WRKJOB" } else { "DSPJOB" };
    let mut history = Vec::new();
    loop {
        match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Return(n) if n > 100 => {
                detail = fetch(host, &q)?;
                if let Left::Exit = option(job, host, surface, &mut d, &detail, n - 100)? {
                    return Ok(String::new());
                }
            }
            Response::Return(0) => {}
            other => {
                if let Flow::Leave = common(job, host, &mut d, &mut history, other) {
                    return Ok(String::new());
                }
            }
        }
    }
}

/// One option of the menu.
fn option(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    detail: &JobDetail,
    n: u32,
) -> Result<Left, Fail> {
    let q = super::qual_job(detail.job.number, &detail.job.user, &detail.job.name);
    let left = match n {
        1..=3 => {
            let panel = ["STSA", "DFNA", "RUNA"][n as usize - 1];
            attributes(host, d, detail);
            d.set("CHGCMD", &format!("CHGJOB JOB({q})"));
            d.page = 0;
            let mut again = |d: &mut UimDialog| {
                if let Ok(fresh) = fetch(host, &q) {
                    attributes(host, d, &fresh);
                }
            };
            show_display(job, host, surface, d, panel, &mut again)?
        }
        4 => spooled(job, host, surface, d, &q)?,
        10 => joblog(job, host, surface, d, detail)?,
        11 => callstack(job, host, surface, d, detail)?,
        13 => libl(job, host, surface, d, detail)?,
        14 => openfiles(job, host, surface, d, detail)?,
        _ => {
            d.message = Some((format!("Option {n} is not available in this release."), true));
            Left::Back
        }
    };
    Ok(left)
}

/// `*NOMAX` for the largest value.
fn nomax(v: u32) -> String {
    if v == u32::MAX {
        "*NOMAX".into()
    } else {
        v.to_string()
    }
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// `LIB/NAME` as `(name, lib)`.
fn split(q: &str) -> (String, String) {
    match q.split_once('/') {
        Some((lib, name)) => (name.to_string(), lib.to_string()),
        None => (q.to_string(), String::new()),
    }
}

/// The type of job as `DSPJOB` names it.
pub(super) fn type_name(kind: char) -> &'static str {
    match kind {
        'A' => "AUTO",
        'B' => "BATCH",
        'I' => "INTER",
        'M' => "SBS",
        'R' => "READER",
        'W' => "WRITER",
        'X' => "SCPF",
        _ => "SYSTEM",
    }
}

/// The status, definition and run attributes.
fn attributes(host: &Host, d: &mut UimDialog, detail: &JobDetail) {
    let j = &detail.job;
    let a = &detail.attrs;
    let now = host.link.borrow_mut().with(Client::now);
    let mine = host.attrs();
    let (date, time) = date_time(j.started, now.offset_min, &mine.datfmt, &mine.datsep);
    let pool = host
        .link
        .borrow_mut()
        .with(Client::pools)
        .into_iter()
        .find(|p| p.name == j.pool)
        .map_or_else(String::new, |p| p.number.to_string());
    let (desc, desc_lib) = split(&a.jobd);
    let (queue, queue_lib) = split(&a.jobq);
    let (outq, outqlib) = split(&a.outq);
    let values: Vec<(&str, String)> = vec![
        ("STATUS", "ACTIVE".into()),
        ("CURUSER", j.user.clone()),
        ("IDENTITY", j.user.clone()),
        ("SETBY", "*DEFAULT".into()),
        ("ENTDATE", date.clone()),
        ("ENTTIME", time.clone()),
        ("STRDATE", date),
        ("STRTIME", time),
        ("SBS", j.subsystem.clone()),
        (
            "SBSPOOL",
            if detail.sbs_pool == 0 {
                String::new()
            } else {
                detail.sbs_pool.to_string()
            },
        ),
        ("TYPE", type_name(j.kind).into()),
        ("SPCENV", "*NONE".into()),
        ("RTNCODE", "0".into()),
        ("CNTRLD", "*NO".into()),
        ("SYSPOOL", pool),
        ("CPUMS", detail.cpu_ms.to_string()),
        ("AUXIO", detail.aux_io.to_string()),
        ("JOBD", desc),
        ("JOBDLIB", desc_lib),
        ("JOBQ", queue),
        ("JOBQLIB", queue_lib),
        ("JOBPTY", a.jobpty.to_string()),
        ("OUTPTY", a.outpty.to_string()),
        ("ENDSEV", a.endsev.to_string()),
        ("LOGLVL", a.log_level.to_string()),
        ("LOGSEV", a.log_sev.to_string()),
        ("LOGTXT", a.log_text.clone()),
        ("LOGCLPGM", a.logclpgm.clone()),
        ("LOGOUT", a.logoutput.clone()),
        ("PRTDEV", a.prtdev.clone()),
        ("OUTQ", outq),
        ("OUTQLIB", outqlib),
        ("PRTTXT", a.prttxt.clone()),
        ("DATFMT", a.datfmt.clone()),
        ("DATSEP", a.datsep.clone()),
        ("TIMSEP", a.timsep.clone()),
        (
            "DECFMT",
            if a.decfmt.trim().is_empty() {
                "*BLANK".into()
            } else {
                a.decfmt.clone()
            },
        ),
        ("SWS", a.sws.clone()),
        ("INQMSG", a.inqmsgrpy.clone()),
        ("BRKMSG", a.brkmsg.clone()),
        ("STSMSG", a.stsmsg.clone()),
        ("DEVRCY", a.devrcyacn.clone()),
        ("CCSID", a.ccsid.to_string()),
        ("DFTCCSID", a.dftccsid.to_string()),
        ("LANGID", a.langid.clone()),
        ("CNTRYID", a.cntryid.clone()),
        ("SRTSEQ", a.srtseq.clone()),
        ("MSGQMX", a.jobmsgqmx.to_string()),
        ("MSGQFL", a.jobmsgqfl.clone()),
        ("ALWMLT", yes(a.alwmltthd).into()),
        ("SPLFACN", a.splfacn.clone()),
        ("RUNPTY", a.runpty.to_string()),
        ("TIMESLICE", a.timeslice.to_string()),
        ("PURGE", yes(a.purge).into()),
        ("DFTWAIT", nomax(a.dftwait)),
        ("CPUTIME", nomax(a.cputime)),
        ("CPUUSED", detail.cpu_ms.to_string()),
        ("MAXTMP", nomax(a.maxtmpstg)),
        ("TMPUSED", detail.tmp_mb.to_string()),
        ("TMPPEAK", detail.tmp_mb.to_string()),
        ("MAXTHD", nomax(a.maxthd)),
        ("THREADS", detail.threads.to_string()),
        ("AFFGRP", "*NOGROUP".into()),
        ("AFFLVL", "*NORMAL".into()),
        ("RSCAFF", "*NO".into()),
    ];
    for (k, v) in values {
        d.set(k, &v);
    }
}

/// Option 4: the job's spooled files.
fn spooled(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, q: &str) -> Result<Left, Fail> {
    let mut history = Vec::new();
    loop {
        let files = host
            .link
            .borrow_mut()
            .with(|l| l.splf_list(String::new(), q.to_string()));
        let rows = files
            .iter()
            .map(|f| {
                let (_, queue) = split(&f.outq);
                entry(&[
                    ("FILE", f.name.clone()),
                    (
                        "DEVQ",
                        if f.writer.is_empty() { queue } else { f.writer.clone() },
                    ),
                    ("USRDTA", f.usrdta.clone()),
                    ("SPLSTS", f.status.code().to_string()),
                    ("PAGES", f.pages.to_string()),
                    ("CURPAGE", f.last_page.to_string()),
                    ("COPIES", f.copies_left.max(1).to_string()),
                    ("SPLNBR", f.number.to_string()),
                ])
            })
            .collect();
        refill(d, "SPLF", rows);
        loop {
            let flow = match d.show(surface, "SPLF").map_err(|e| render_fail(&e))? {
                Response::Exit => return Ok(Left::Exit),
                Response::Cancel | Response::Return(16) => return Ok(Left::Back),
                Response::Options(choices) => {
                    for (n, c) in run_options(job, host, d, &mut history, &choices) {
                        d.message =
                            Some((format!("Option {} ({n}) is not available yet.", c.option), true));
                    }
                    Flow::Refresh
                }
                other => common(job, host, d, &mut history, other),
            };
            match flow {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(Left::Back),
            }
        }
    }
}

/// The job log's lines: requests with their marks, messages under
/// them; `all` keeps the detailed ones too.
fn log_lines(job: &Job, host: &Host, all: bool) -> Vec<Vec<(String, String)>> {
    let a = host.attrs();
    let entries: Vec<&wm::joblog::Entry> = job.joblog.entries().collect();
    let kept = if all {
        entries
    } else {
        wm::joblog::filter(&entries, a.log_level.max(1), u16::from(a.log_sev))
    };
    let last_request = kept.iter().rposition(|e| e.kind == "Request");
    kept.iter()
        .enumerate()
        .map(|(i, e)| {
            let line = if e.kind == "Request" {
                let mark = if Some(i) == last_request { ">>" } else { "> " };
                format!("3{mark} {}", e.text.trim())
            } else {
                format!("    {}", e.text.trim())
            };
            entry(&[("MSGLINE", line)])
        })
        .collect()
}

/// Option 10 and `DSPJOBLOG`: the job's log, the basic view first.
fn joblog(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, detail: &JobDetail) -> Result<Left, Fail> {
    if !own(host, detail) {
        d.message = Some((
            "The log of another job is shown once it ends (WRKSPLF).".into(),
            true,
        ));
        return Ok(Left::Back);
    }
    let mut all = false;
    let mut history = Vec::new();
    loop {
        let rows = log_lines(job, host, all);
        refill(d, "MSGS", rows);
        let panel = if all { "JOBLOGALL" } else { "JOBLOG" };
        let flow = match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Exit => return Ok(Left::Exit),
            Response::Cancel | Response::Return(1 | 16) => return Ok(Left::Back),
            Response::Return(10) => {
                all = true;
                Flow::Refresh
            }
            other => common(job, host, d, &mut history, other),
        };
        if let Flow::Leave = flow {
            return Ok(Left::Back);
        }
    }
}

/// Option 11: the call stack, command entry at the bottom.
fn callstack(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, detail: &JobDetail) -> Result<Left, Fail> {
    if !own(host, detail) {
        d.message = Some(("The call stack of another job is not shown yet.".into(), true));
        return Ok(Left::Back);
    }
    d.set("THREAD", &format!("{:08X}", detail.job.pid));
    let mut rows = vec![entry(&[
        ("PGM", "QCMD".into()),
        ("PGMLIB", "QSYS".into()),
    ])];
    let top = job.act.frames.len();
    for (i, f) in job.act.frames.iter().enumerate() {
        let key = f.program.as_ref().map(|p| p.key.clone()).unwrap_or_default();
        let (lib, name) = key.split_once('/').unwrap_or(("", key.as_str()));
        let proc_name = f
            .program
            .as_ref()
            .and_then(|p| p.proc_name(f.code))
            .unwrap_or_default();
        let stmt = if i + 1 == top { job.abi.cur_stmt } else { f.stmt };
        rows.push(entry(&[
            ("RQSLVL", String::new()),
            ("PGM", name.to_string()),
            ("PGMLIB", lib.to_string()),
            ("STMT", stmt.to_string()),
            ("PROC", proc_name),
        ]));
    }
    refill(d, "STACK", rows);
    simple(job, host, surface, d, "CALLSTK")
}

/// Option 13: the library list with each library's text.
fn libl(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, detail: &JobDetail) -> Result<Left, Fail> {
    let l = &detail.attrs.libl;
    let mut libs: Vec<(String, &str)> = Vec::new();
    libs.extend(l.sys.iter().map(|x| (x.clone(), "SYS")));
    libs.extend(l.prd.iter().filter(|x| !x.is_empty()).map(|x| (x.clone(), "PRD")));
    libs.extend(l.cur.iter().map(|x| (x.clone(), "CUR")));
    libs.extend(l.usr.iter().map(|x| (x.clone(), "USR")));
    let rows = libs
        .into_iter()
        .map(|(lib, kind)| {
            let text = host
                .link
                .borrow_mut()
                .with(|c| c.object("QSYS".into(), lib.clone(), "*LIB".into()))
                .map(|o| o.text)
                .unwrap_or_default();
            entry(&[
                ("LIB", lib),
                ("LIBTYPE", kind.to_string()),
                ("ASPDEV", String::new()),
                ("LIBTEXT", text),
            ])
        })
        .collect();
    refill(d, "LIBL", rows);
    simple(job, host, surface, d, "LIBL")
}

/// Option 14: the job's open files.
fn openfiles(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, detail: &JobDetail) -> Result<Left, Fail> {
    let rows: Vec<Vec<(String, String)>> = if own(host, detail) {
        job.files
            .values()
            .map(|f| {
                entry(&[
                    ("OFILE", f.info.file.clone()),
                    ("OLIB", f.info.lib.clone()),
                    ("OMBR", f.info.member.clone()),
                    ("OSCOPE", "*ACTGRPDFN".into()),
                    ("OACTGRP", String::new()),
                ])
            })
            .collect()
    } else {
        Vec::new()
    };
    d.set("ODPS", &rows.len().to_string());
    let j = &detail.job;
    let saved = d.get("JOBHDR");
    d.set(
        "JOBHDR",
        &format!(
            "Job . . :   {:<10}     User . . :   {:<10}     Number . . . :   {:06}",
            j.name, j.user, j.number
        ),
    );
    refill(d, "OPNF", rows);
    let left = simple(job, host, surface, d, "OPNF");
    d.set("JOBHDR", &saved);
    left
}

/// A list display without options: F3 leaves, Enter, F12 and F16
/// go back to the menu, list options run as typed.
fn simple(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, panel: &str) -> Result<Left, Fail> {
    let mut history = Vec::new();
    loop {
        match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Exit | Response::Gone => return Ok(Left::Exit),
            Response::Return(5) => {}
            Response::Cancel | Response::Return(_) => return Ok(Left::Back),
            other => {
                if let Flow::Leave = common(job, host, d, &mut history, other) {
                    return Ok(Left::Back);
                }
            }
        }
    }
}
