// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKJOBD`/`DSPJOBD` (`QUIWRKJOBD`) and `WRKCLS`/`DSPCLS`
//! (`QUIWRKCLS`): the work management objects of a library and what one
//! holds.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use wm::obj::cls::Cls;
use wm::obj::jobd::JobD;

use super::{Flow, common, entry, qualified, refill, render_fail, show_display};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// One kind of object a `WRK` list shows.
struct Kind {
    group: &'static str,
    panel: &'static str,
    list: &'static str,
    var: &'static str,
    ty: &'static str,
    kwd: &'static str,
}

const JOBD: Kind = Kind {
    group: "QUIWRKJOBD",
    panel: "WRKJOBD",
    list: "JOBDS",
    var: "JOBD",
    ty: "*JOBD",
    kwd: "JOBD",
};

const CLS: Kind = Kind {
    group: "QUIWRKCLS",
    panel: "WRKCLS",
    list: "CLASSES",
    var: "CLS",
    ty: "*CLS",
    kwd: "CLS",
};

/// `WRKJOBD [JOBD(lib/name)]`.
pub fn wrkjobd(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    work(job, host, surface, r, &JOBD)
}

/// `WRKCLS [CLS(lib/name)]`.
pub fn wrkcls(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    work(job, host, surface, r, &CLS)
}

fn work(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, k: &Kind) -> Result<String, Fail> {
    let (lib, name) = qualified(r, k.kwd).unwrap_or_else(|| ("*LIBL".into(), "*ALL".into()));
    let mut d = crate::panels::dialog(host, k.group)?;
    let mut history = Vec::new();
    loop {
        let objs = host
            .link
            .borrow_mut()
            .with(|l| l.list_objects(lib.clone(), name.clone(), k.ty.into()));
        // The first line is where option 1 types a new name.
        let mut rows = vec![entry(&[(k.var, String::new()), ("LIB", String::new())])];
        rows.extend(objs.into_iter().map(|o| {
            entry(&[(k.var, o.name), ("LIB", o.lib), ("TEXT", o.text)])
        }));
        refill(&mut d, k.list, rows);
        loop {
            let flow = match d.show(surface, k.panel).map_err(|e| render_fail(&e))? {
                Response::Return(11 | 16 | 17) => {
                    d.message = Some(("That function is not available in this release.".into(), true));
                    Flow::Stay
                }
                other => common(job, host, &mut d, &mut history, other),
            };
            match flow {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(String::new()),
            }
        }
    }
}

/// A work management object's payload and its real library.
fn fetch(host: &Host, r: &Resolved, kwd: &str, ty: &str) -> Result<(Vec<u8>, String, String), Fail> {
    let (lib, name) = qualified(r, kwd).ok_or_else(|| Fail::new("CPD0071", kwd))?;
    let payload = host
        .link
        .borrow_mut()
        .with(|l| l.wm_object(format!("{lib}/{name}"), ty.into()))
        .map_err(|e| {
            let (id, text) = e.split_once(' ').unwrap_or(("CPF9801", e.as_str()));
            Fail::new(id, text)
        })?;
    let real = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), ty.into()))
        .map_or(lib, |o| o.lib);
    Ok((payload, real, name))
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

fn nomax(v: Option<u32>) -> String {
    v.map_or_else(|| "*NOMAX".to_string(), |n| n.to_string())
}

/// `DSPJOBD JOBD(lib/name)`.
pub fn dspjobd(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (payload, lib, name) = fetch(host, r, "JOBD", "*JOBD")?;
    let j = JobD::decode(&payload).map_err(|e| Fail::new("CPF9801", e.to_string()))?;
    let mut d = crate::panels::dialog(host, "QUIWRKJOBD")?;
    let values: Vec<(&str, String)> = vec![
        ("JOBD", name),
        ("LIB", lib),
        ("USER", j.user.clone()),
        ("SYNTAX", j.syntax.map_or_else(|| "*NOCHK".into(), |s| s.to_string())),
        ("HOLD", yes(j.hold).into()),
        ("ENDSEV", j.endsev.to_string()),
        ("JOBDATE", "*SYSVAL".into()),
        ("SWS", "00000000".into()),
        ("INQMSG", j.inqmsgrpy.clone()),
        ("JOBPTY", j.jobpty.to_string()),
        ("JOBQ", j.jobq.1.clone()),
        ("JOBQLIB", j.jobq.0.clone()),
        ("OUTPTY", j.outpty.to_string()),
        ("PRTDEV", j.prtdev.clone()),
        ("OUTQ", j.outq.1.clone()),
        ("OUTQLIB", j.outq.0.clone()),
        ("LOGLVL", j.log_level.to_string()),
        ("LOGSEV", j.log_sev.to_string()),
        ("LOGTXT", j.log_text.clone()),
        ("LOGCLPGM", j.logclpgm.clone()),
        ("LOGOUT", j.logoutput.clone()),
        ("ACGCDE", "*USRPRF".into()),
        ("PRTTXT", j.prttxt.clone()),
        ("RTGDTA", j.rtgdta.clone()),
        ("RQSDTA", j.rqsdta.clone()),
        ("DEVRCY", j.devrcyacn.clone()),
        ("TSEPOOL", j.tsepool.clone()),
        ("MSGQMX", j.jobmsgqmx.map_or_else(|| "*SYSVAL".into(), |m| m.to_string())),
        ("MSGQFL", j.jobmsgqfl.clone()),
        ("ALWMLT", yes(j.alwmltthd).into()),
        ("ASPGRP", j.inlaspgrp.clone()),
        ("SPLFACN", j.splfacn.clone()),
        ("DDMCNV", j.ddmcnv.clone()),
        ("TEXT", j.text.clone()),
        (
            "INLLIBL",
            j.inllibl.as_ref().map_or_else(|| "*SYSVAL".into(), |l| l.join(" ")),
        ),
    ];
    for (k, v) in values {
        d.set(k, &v);
    }
    display(job, host, surface, &mut d, "DSPJOBD")
}

/// `DSPCLS CLS(lib/name)`.
pub fn dspcls(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (payload, lib, name) = fetch(host, r, "CLS", "*CLS")?;
    let c = Cls::decode(&payload).map_err(|e| Fail::new("CPF9801", e.to_string()))?;
    let mut d = crate::panels::dialog(host, "QUIWRKCLS")?;
    let values: Vec<(&str, String)> = vec![
        ("CLS", name),
        ("LIB", lib),
        ("RUNPTY", c.runpty.to_string()),
        ("TIMESLICE", c.timeslice.to_string()),
        ("PURGE", yes(c.purge).into()),
        ("DFTWAIT", nomax(c.dftwait)),
        ("CPUTIME", nomax(c.cputime)),
        ("MAXTMP", nomax(c.maxtmpstg)),
        ("MAXTHD", nomax(c.maxthd)),
        ("TEXT", c.text.clone()),
    ];
    for (k, v) in values {
        d.set(k, &v);
    }
    display(job, host, surface, &mut d, "DSPCLS")
}

/// A display panel until it is left.
fn display(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, panel: &str) -> Result<String, Fail> {
    show_display(job, host, surface, d, panel, &mut |_| {})?;
    Ok(String::new())
}
