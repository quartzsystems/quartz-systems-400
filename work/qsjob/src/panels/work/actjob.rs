// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKACTJOB` (`QWCWRKAJ`): the active jobs under their subsystems in
//! three views, measured from the display's start or the last F10.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::proto::ActJobs;
use wm::job::wrkactjob_type;

use super::{Flow, common, decimal, entry, parm, refill, render_fail, stamp};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

const LIST: &str = "JOBS";

/// `WRKACTJOB [RESET(*YES)] [SBS(...)]`.
pub fn wrkactjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QWCWRKAJ")?;
    let sbs: Vec<String> = parm(r, "SBS")
        .filter(|s| s != "*ALL")
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let mut panel = "WRKACTJ1";
    let mut history = Vec::new();
    let mut advance = true;
    loop {
        let data = host.link.borrow_mut().with(|l| l.actjob_data(advance));
        advance = false;
        fill(host, &mut d, &data, &sbs);
        loop {
            let resp = d.show(surface, panel).map_err(|e| render_fail(&e))?;
            let flow = match resp {
                Response::Return(10) => {
                    advance = true;
                    Flow::Refresh
                }
                Response::Return(11) => {
                    d.view = (d.view + 1) % 3;
                    d.set("VIEW", &(d.view + 1).to_string());
                    Flow::Stay
                }
                Response::Return(23) => {
                    panel = if panel == "WRKACTJ1" { "WRKACTJ2" } else { "WRKACTJ1" };
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

/// The header and the entries.
fn fill(host: &Host, d: &mut UimDialog, data: &ActJobs, sbs: &[String]) {
    d.set("*DATETIME", &stamp(host));
    d.set("SYSCPU", &decimal(host, u64::from(data.cpu)));
    d.set("ELAPSED", &wm::stats::hms(data.elapsed));
    let shown: Vec<_> = data
        .jobs
        .iter()
        .filter(|a| {
            sbs.is_empty()
                || sbs.iter().any(|s| {
                    *s == a.job.subsystem || (a.job.kind == 'M' && *s == a.job.name)
                })
        })
        .collect();
    d.set("ACTJOBS", &shown.len().to_string());
    let rows = shown
        .into_iter()
        .map(|a| {
            let j = &a.job;
            let kind = j
                .job_type()
                .map_or("   ", |t| wrkactjob_type(t, j.sub_type()));
            let name = if a.under {
                format!("  {}", j.name)
            } else {
                j.name.clone()
            };
            entry(&[
                ("JOB", name),
                ("JOBNM", j.name.clone()),
                ("USER", j.user.clone()),
                ("CURUSER", j.user.clone()),
                ("NUMBER", format!("{:06}", j.number)),
                ("TYPE", kind.to_string()),
                ("CPU", decimal(host, u64::from(a.cpu))),
                ("FUNCTION", j.function.clone()),
                // The status sits one column into its heading.
                ("STATUS", format!(" {}", j.job_status().code())),
                ("POOL", a.pool.to_string()),
                ("PTY", a.runpty.to_string()),
                ("CPUSEC", decimal(host, a.cpu_secs)),
                ("INT", String::new()),
                ("RSP", String::new()),
                ("AUXIO", a.aux_io.to_string()),
                ("ECPU", decimal(host, u64::from(a.cpu))),
                ("THREADS", a.threads.to_string()),
                ("TMPSTG", a.tmp_mb.to_string()),
            ])
        })
        .collect();
    refill(d, LIST, rows);
}
