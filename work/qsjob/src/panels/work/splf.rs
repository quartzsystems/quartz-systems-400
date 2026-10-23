// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKSPLF` (`QUIWRKSPLF`): the spooled files of a user, of a job or
//! of everyone, oldest first.

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use super::{Flow, common, entry, parm, refill, render_fail, run_options};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `WRKSPLF [SELECT(user ...)] [JOB()]`.
pub fn wrksplf(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    // The first element of SELECT is the user.
    let select = r.get("SELECT").and_then(|b| b.value.as_ref()).and_then(|v| match v {
        Value::Elems(items) => items.first().cloned().flatten(),
        other => Some(other.clone()),
    });
    let user = match select.map(|v| v.source().to_ascii_uppercase()).as_deref() {
        None | Some("*CURRENT") => host.start.user.clone(),
        Some("*ALL") => String::new(),
        Some(u) => u.to_string(),
    };
    let jobq = parm(r, "JOB").filter(|j| j != "*").unwrap_or_default();
    let mut d = crate::panels::dialog(host, "QUIWRKSPLF")?;
    let mut history = Vec::new();
    loop {
        let files = host
            .link
            .borrow_mut()
            .with(|l| l.splf_list(user.clone(), jobq.clone()));
        let rows = files
            .iter()
            .map(|f| {
                let queue = f.outq.rsplit('/').next().unwrap_or("").to_string();
                entry(&[
                    ("FILE", f.name.clone()),
                    ("USER", f.job.1.clone()),
                    ("DEVQ", if f.writer.is_empty() { queue } else { f.writer.clone() }),
                    ("USRDTA", f.usrdta.clone()),
                    ("STS", f.status.code().to_string()),
                    ("PAGES", f.pages.to_string()),
                    ("CURPAGE", if f.last_page == 0 { String::new() } else { f.last_page.to_string() }),
                    ("COPIES", f.copies_left.max(1).to_string()),
                    ("JOB", f.job.0.clone()),
                    ("NUMBER", format!("{:06}", f.job.2)),
                    ("SPLNBR", f.number.to_string()),
                ])
            })
            .collect();
        refill(&mut d, "FILES", rows);
        loop {
            let flow = match d.show(surface, "WRKSPLF").map_err(|e| render_fail(&e))? {
                Response::Options(choices) => {
                    for (n, c) in run_options(job, host, &mut d, &mut history, &choices) {
                        d.message = Some((format!("Option {} ({n}) is not available yet.", c.option), true));
                    }
                    Flow::Refresh
                }
                Response::Return(10 | 11) => {
                    d.message = Some(("That view is not available in this release.".into(), true));
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
