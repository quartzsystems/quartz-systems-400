// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKUSRJOB` and `WRKSBSJOB` (`QUIWRKUSRJ`): the jobs of a user or
//! of a subsystem, active ones and ended ones with spooled output.

use std::collections::BTreeSet;

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::job::type_name;
use super::{Flow, common, entry, parm, refill, render_fail, stamp};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

const LIST: &str = "JOBS";

/// Which jobs a display lists.
struct Select {
    user: Option<String>,
    sbs: Option<String>,
    active: bool,
    outq: bool,
}

/// `WRKUSRJOB [USER()] [STATUS()]`.
pub fn wrkusrjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let user = match parm(r, "USER").as_deref() {
        None | Some("*") => Some(host.start.user.clone()),
        Some("*ALL") => None,
        Some(u) => Some(u.to_string()),
    };
    let status = parm(r, "STATUS").unwrap_or_else(|| "*ALL".into());
    let sel = Select {
        user,
        sbs: None,
        active: matches!(status.as_str(), "*ALL" | "*ACTIVE"),
        outq: matches!(status.as_str(), "*ALL" | "*OUTQ"),
    };
    let mut d = crate::panels::dialog(host, "QUIWRKUSRJ")?;
    show(job, host, surface, &mut d, "WRKUSRJOB", &sel)
}

/// `WRKSBSJOB [SBS()]`.
pub fn wrksbsjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let sbs = parm(r, "SBS").unwrap_or_else(|| "*ALL".into());
    let mut d = crate::panels::dialog(host, "QUIWRKUSRJ")?;
    d.set("SBS", &sbs);
    let sel = Select {
        user: None,
        sbs: (sbs != "*ALL").then_some(sbs),
        active: true,
        outq: false,
    };
    show(job, host, surface, &mut d, "WRKSBSJOB", &sel)
}

fn show(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    sel: &Select,
) -> Result<String, Fail> {
    let mut history = Vec::new();
    loop {
        fill(host, d, sel);
        loop {
            let flow = match d.show(surface, panel).map_err(|e| render_fail(&e))? {
                Response::Return(11 | 21) => {
                    d.message = Some(("That view is not available in this release.".into(), true));
                    Flow::Stay
                }
                other => common(job, host, d, &mut history, other),
            };
            match flow {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(String::new()),
            }
        }
    }
}

/// The type of an ended job from its name alone.
fn ended_type(name: &str) -> &'static str {
    if name == "QPRTJOB" {
        "PRINT"
    } else if name.starts_with("QPADEV") {
        "INTER"
    } else {
        "BATCH"
    }
}

fn fill(host: &Host, d: &mut UimDialog, sel: &Select) {
    d.set("*DATETIME", &stamp(host));
    let mut active = host.link.borrow_mut().with(Client::active_jobs);
    active.retain(|j| {
        sel.user.as_ref().is_none_or(|u| j.user == *u)
            && sel.sbs.as_ref().is_none_or(|s| j.subsystem == *s)
            && (sel.sbs.is_some() || sel.user.is_some() || j.kind != 'S')
    });
    active.sort_by(|a, b| a.name.cmp(&b.name).then(a.number.cmp(&b.number)));
    let mut rows = Vec::new();
    let numbers: BTreeSet<u32> = active.iter().map(|j| j.number).collect();
    if sel.active {
        for j in &active {
            rows.push(entry(&[
                ("JOB", j.name.clone()),
                ("USER", j.user.clone()),
                ("NUMBER", format!("{:06}", j.number)),
                ("TYPE", type_name(j.kind).to_string()),
                ("STATUS", "ACTIVE".to_string()),
                ("FUNCTION", j.function.clone()),
            ]));
        }
    }
    if sel.outq {
        let user = sel.user.clone().unwrap_or_default();
        let files = host
            .link
            .borrow_mut()
            .with(|l| l.splf_list(user, String::new()));
        let mut ended: BTreeSet<(String, String, u32)> = BTreeSet::new();
        for f in files {
            if !numbers.contains(&f.job.2) {
                ended.insert(f.job.clone());
            }
        }
        for (name, user, number) in ended {
            rows.push(entry(&[
                ("TYPE", ended_type(&name).to_string()),
                ("JOB", name),
                ("USER", user),
                ("NUMBER", format!("{number:06}")),
                ("STATUS", "OUTQ".to_string()),
                ("FUNCTION", String::new()),
            ]));
        }
    }
    refill(d, LIST, rows);
}
