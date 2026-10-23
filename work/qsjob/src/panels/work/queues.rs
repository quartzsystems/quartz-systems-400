// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKOUTQ` (`QUIWRKOUTQ`), `WRKJOBQ` (`QUIWRKJOBQ`), `WRKWTR`
//! (`QUIWRKWTR`) and `WRKJOBSCDE` (`QUIWRKJSCD`): the queues, the
//! writers and the job schedule.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use wm::obj::jobq::JobQ;
use wm::obj::jobscd::{Frequency, JobScd, SCHEDULE};
use wm::obj::outq::OutQ;

use super::{Flow, common, date_time, entry, parm, qualified, refill, render_fail};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::UimDialog;

/// Show a list panel, `fill` before each look, until it is left.
fn list_loop(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    fill: &mut dyn FnMut(&Host, &mut UimDialog),
) -> Result<String, Fail> {
    let mut history = Vec::new();
    loop {
        fill(host, d);
        loop {
            let resp = d.show(surface, panel).map_err(|e| render_fail(&e))?;
            match common(job, host, d, &mut history, resp) {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(String::new()),
            }
        }
    }
}

/// Every object of a type, as `(lib, name)`.
fn all_of(host: &Host, ty: &str) -> Vec<(String, String)> {
    host.link
        .borrow_mut()
        .with(|l| l.list_objects("*ALL".into(), "*ALL".into(), ty.into()))
        .into_iter()
        .map(|o| (o.lib, o.name))
        .collect()
}

fn payload(host: &Host, lib: &str, name: &str, ty: &str) -> Option<Vec<u8>> {
    host.link
        .borrow_mut()
        .with(|l| l.wm_object(format!("{lib}/{name}"), ty.into()))
        .ok()
}

fn held(h: bool) -> &'static str {
    if h { "HLD" } else { "RLS" }
}

/// `WRKOUTQ [OUTQ(lib/name)]`.
pub fn wrkoutq(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QUIWRKOUTQ")?;
    match qualified(r, "OUTQ").filter(|(_, n)| n != "*ALL") {
        None => list_loop(job, host, surface, &mut d, "QUEUES", &mut |host, d| {
            let rows = all_of(host, "*OUTQ")
                .into_iter()
                .map(|(lib, name)| {
                    let q = payload(host, &lib, &name, "*OUTQ").and_then(|b| OutQ::decode(&b).ok());
                    entry(&[
                        ("FILES", q.as_ref().map_or(0, |q| q.files.len()).to_string()),
                        ("WRITER", q.as_ref().map(|q| q.writer.clone()).unwrap_or_default()),
                        ("STATUS", held(q.as_ref().is_some_and(|q| q.held)).into()),
                        ("QUEUE", name),
                        ("LIB", lib),
                    ])
                })
                .collect();
            refill(d, "QUEUES", rows);
        }),
        Some((lib, name)) => {
            let real = host
                .link
                .borrow_mut()
                .with(|l| l.object(lib.clone(), name.clone(), "*OUTQ".into()))
                .ok_or_else(|| Fail::new("CPF3357", format!("Output queue {name} in {lib} not found.")))?
                .lib;
            d.set("QUEUE", &name);
            d.set("LIB", &real);
            let qual = format!("{real}/{name}");
            list_loop(job, host, surface, &mut d, "FILES", &mut |host, d| {
                let q = payload(host, &real, &name, "*OUTQ").and_then(|b| OutQ::decode(&b).ok());
                d.set("QSTS", held(q.is_some_and(|q| q.held)));
                let rows = host
                    .link
                    .borrow_mut()
                    .with(|l| l.splf_list(String::new(), String::new()))
                    .into_iter()
                    .filter(|f| f.outq == qual)
                    .map(|f| {
                        entry(&[
                            ("FILE", f.name.clone()),
                            ("USER", f.job.1.clone()),
                            ("USRDTA", f.usrdta.clone()),
                            ("STS", f.status.code().to_string()),
                            ("PAGES", f.pages.to_string()),
                            ("COPIES", f.copies_left.max(1).to_string()),
                            ("JOB", f.job.0.clone()),
                            ("NUMBER", format!("{:06}", f.job.2)),
                            ("SPLNBR", f.number.to_string()),
                        ])
                    })
                    .collect();
                refill(d, "FILES", rows);
            })
        }
    }
}

/// `WRKJOBQ [JOBQ(lib/name)]`.
pub fn wrkjobq(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QUIWRKJOBQ")?;
    match qualified(r, "JOBQ").filter(|(_, n)| n != "*ALL") {
        None => list_loop(job, host, surface, &mut d, "QUEUES", &mut |host, d| {
            let attach = host.link.borrow_mut().with(Client::jobq_attach);
            let rows = all_of(host, "*JOBQ")
                .into_iter()
                .map(|(lib, name)| {
                    let q = payload(host, &lib, &name, "*JOBQ").and_then(|b| JobQ::decode(&b).ok());
                    let qual = format!("{lib}/{name}");
                    let sbs = attach
                        .iter()
                        .find(|(k, _)| *k == qual)
                        .map(|(_, s)| s.clone())
                        .unwrap_or_default();
                    entry(&[
                        ("JOBS", q.as_ref().map_or(0, |q| q.jobs.len()).to_string()),
                        ("STATUS", held(q.as_ref().is_some_and(|q| q.held)).into()),
                        ("SBS", sbs),
                        ("QUEUE", name),
                        ("LIB", lib),
                    ])
                })
                .collect();
            refill(d, "QUEUES", rows);
        }),
        Some((lib, name)) => {
            let real = host
                .link
                .borrow_mut()
                .with(|l| l.object(lib.clone(), name.clone(), "*JOBQ".into()))
                .ok_or_else(|| Fail::new("CPF3307", format!("Job queue {name} in {lib} not found.")))?
                .lib;
            d.set("QUEUE", &name);
            d.set("LIB", &real);
            list_loop(job, host, surface, &mut d, "JOBS", &mut |host, d| {
                let now = host.link.borrow_mut().with(Client::now).unix;
                let q = payload(host, &real, &name, "*JOBQ").and_then(|b| JobQ::decode(&b).ok());
                d.set("QSTS", held(q.as_ref().is_some_and(|q| q.held)));
                let rows = q
                    .map(|q| q.jobs)
                    .unwrap_or_default()
                    .iter()
                    .map(|j| {
                        entry(&[
                            ("JOB", j.name.clone()),
                            ("USER", j.user.clone()),
                            ("NUMBER", format!("{:06}", j.number)),
                            ("PTY", j.jobpty.to_string()),
                            ("JSTS", j.status(now).to_string()),
                        ])
                    })
                    .collect();
                refill(d, "JOBS", rows);
            })
        }
    }
}

/// `WRKWTR [WTR()]`.
pub fn wrkwtr(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let only = parm(r, "WTR").filter(|w| w != "*ALL");
    let mut d = crate::panels::dialog(host, "QUIWRKWTR")?;
    list_loop(job, host, surface, &mut d, "WRKWTR", &mut |host, d| {
        let rows = host
            .link
            .borrow_mut()
            .with(Client::wtr_list)
            .into_iter()
            .filter(|w| only.as_ref().is_none_or(|o| w.0 == *o))
            .map(|(name, dev, status, form, outq, printed)| {
                let (lib, q) = outq.split_once('/').map_or((String::new(), outq.clone()), |(l, q)| {
                    (l.to_string(), q.to_string())
                });
                entry(&[
                    ("WRITER", name),
                    ("DEVICE", dev),
                    ("STATUS", status),
                    ("FORMTYPE", form),
                    ("OUTQ", q),
                    ("LIB", lib),
                    ("PRINTED", printed.to_string()),
                ])
            })
            .collect();
        refill(d, "WRITERS", rows);
    })
}

/// `WRKJOBSCDE [JOB()]`.
pub fn wrkjobscde(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let only = parm(r, "JOB").filter(|j| j != "*ALL");
    let mut d = crate::panels::dialog(host, "QUIWRKJSCD")?;
    list_loop(job, host, surface, &mut d, "WRKJOBSCDE", &mut |host, d| {
        let (lib, name) = SCHEDULE;
        let scd = payload(host, lib, name, "*JOBSCD")
            .and_then(|b| JobScd::decode(&b).ok())
            .unwrap_or_default();
        let now = host.link.borrow_mut().with(Client::now);
        let a = host.attrs();
        let rows = scd
            .entries
            .iter()
            .filter(|e| {
                only.as_ref().is_none_or(|o| match o.strip_suffix('*') {
                    Some(prefix) => e.job.starts_with(prefix),
                    None => e.job == *o,
                })
            })
            .map(|e| {
                let (date, time) = if e.next == 0 {
                    (String::new(), String::new())
                } else {
                    date_time(e.next, now.offset_min, &a.datfmt, &a.datsep)
                };
                let frq = match e.frequency {
                    Frequency::Once => "*ONCE",
                    Frequency::Weekly => "*WEEKLY",
                    Frequency::Monthly => "*MONTHLY",
                };
                entry(&[
                    ("JOB", e.job.clone()),
                    ("STATUS", if e.held { "HLD" } else { "SCD" }.into()),
                    ("SDATE", date),
                    ("STIME", time),
                    ("FRQ", frq.into()),
                    ("TEXT", e.text.clone()),
                    ("ENTRY", format!("{:06}", e.number)),
                ])
            })
            .collect();
        refill(d, "ENTRIES", rows);
    })
}
