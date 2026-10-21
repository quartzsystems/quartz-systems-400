// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job log at a job's end: the service's copy, printed as the job's
//! `LOG` keeps it into a `QPJOBLOG` spooled file. `*NOLIST` prints
//! nothing unless the job ended abnormally; `LOGOUTPUT(*PND)` leaves it
//! unprinted. `*JOBLOGSVR` prints it here too (`MIGRATE(phase6)`: a job
//! log server).

use spool::{Control, PageFormat, Printer};
use wm::joblog::{self, Header};
use wm::obj::splf::{Schedule, SplfAttrs};

use super::served::JobCtx;
use crate::ipl::system::{System, now_unix};

/// The printer file job logs print through.
pub const PRTF: &str = "QSYS/QPJOBLOG";

/// A job that vanished could not log its own end: the service does.
pub fn ended_here(system: &System, ctx: &mut JobCtx, data: &[u8]) {
    let text = super::hist::message_text("CPF1164", data);
    let _ = system;
    let e = joblog::Entry {
        msgid: "CPF1164".into(),
        kind: "Completion".into(),
        time: super::msgs::now_micros(),
        from: ("QWTMCEOJ".into(), "QSYS".into()),
        from_inst: "*N".into(),
        to: ("*EXT".into(), String::new()),
        to_inst: "*N".into(),
        text,
        external: true,
        ..joblog::Entry::default()
    };
    if let joblog::Appended::Wrapped(gone) = ctx.joblog.append(e) {
        ctx.joblog_wrapped.extend(gone);
    }
}

/// Where the job's log goes: `*JOBEND`, `*JOBLOGSVR` or `*PND`.
fn output(system: &System, attrs: &wm::job::JobAttrs) -> String {
    if attrs.logoutput == "*SYSVAL" || attrs.logoutput.is_empty() {
        system
            .sysvals
            .as_ref()
            .and_then(|v| v.get("QLOGOUTPUT").ok())
            .map_or_else(|| "*JOBEND".into(), |v| v.text())
    } else {
        attrs.logoutput.clone()
    }
}

/// The printed job log's lines, or `None` when nothing is printed.
pub fn lines(system: &System, ctx: &JobCtx, abnormal: bool) -> Option<Vec<String>> {
    let a = system.attrs.get(&ctx.job.number)?;
    if a.log_text == "*NOLIST" && !abnormal {
        return None;
    }
    if output(system, a) == "*PND" {
        return None;
    }
    let (jlib, jname) = a.jobd.split_once('/').unwrap_or(("", a.jobd.as_str()));
    let (date, time) = joblog::stamp_of(now_unix().saturating_mul(1_000_000));
    let j = &ctx.job;
    let header = Header {
        job: (j.name.clone(), j.user.clone(), j.number),
        jobd: (jname.to_string(), jlib.to_string()),
        system: "QS400".into(),
        stamp: (date, time[..8].to_string()),
    };
    let mut all: Vec<&joblog::Entry> = ctx.joblog_wrapped.iter().collect();
    all.extend(ctx.joblog.entries());
    let kept = joblog::filter(&all, a.log_level, u16::from(a.log_sev));
    Some(joblog::print(&header, &kept, a.log_text != "*MSG"))
}

/// Spool the job log as `QPJOBLOG`, if the job's `LOG` says to.
pub fn spool(system: &mut System, ctx: &JobCtx, abnormal: bool) {
    let Some(lines) = lines(system, ctx, abnormal) else {
        return;
    };
    let prtf = super::prtf::resolve(system, PRTF, &[]).unwrap_or_default();
    let fmt = PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: false,
    };
    let mut p = Printer::new(fmt);
    for l in &lines {
        let ctl = if l.starts_with("QS400 V") {
            Control {
                skip_before: 1,
                ..Control::default()
            }
        } else {
            Control::line()
        };
        p.write_str(l, ctl);
    }
    let j = &ctx.job;
    let attrs = SplfAttrs {
        name: "QPJOBLOG".into(),
        job: (j.name.clone(), j.user.clone(), j.number),
        usrdta: j.name.clone(),
        schedule: Schedule::FileEnd,
        copies: prtf.copies.max(1),
        outpty: 5,
        page_len: fmt.len,
        page_width: fmt.width,
        lpi: fmt.lpi,
        cpi: fmt.cpi,
        ovrflw: fmt.ovrflw,
        formtype: prtf.formtype.clone(),
        outq: super::prtf::outq_of(system, &prtf, j.number),
        prtf: PRTF.into(),
        ..SplfAttrs::default()
    };
    let r = super::spool::create(system, attrs).and_then(|id| {
        let data = p.take();
        super::spool::append(system, id, &data, p.pages, p.records)?;
        super::spool::close(system, id, prtf.hold)
    });
    if let Err(e) = r {
        eprintln!("qsstg: job log of {}: {e}", j.name);
    }
}
