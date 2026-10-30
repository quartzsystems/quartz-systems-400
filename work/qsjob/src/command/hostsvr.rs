// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRHOSTSVR` and `ENDHOSTSVR`: the host server daemon jobs start as
//! children of the job that ran the command (as `STRTCPSVR` starts the
//! telnet server), each in the subsystem IBM i runs it in; their
//! prestart jobs come and go with `QUSRWRK` and `QSERVER`.

use std::fmt::Write as _;

use cmd::Resolved;
use hostserver::jobs::{DAEMONS, Daemon};
use mi::Job;
use stg::Client;

use super::Fail;
use crate::console::host::Host;

/// The daemons a `SERVER` list names.
fn selected(r: &Resolved) -> Vec<&'static Daemon> {
    let words: Vec<String> = r
        .parms
        .iter()
        .find(|b| b.kwd == "SERVER")
        .and_then(|b| b.value.as_ref())
        .map_or_else(
            || vec!["*ALL".to_string()],
            |v| {
                v.source()
                    .split_whitespace()
                    .map(|s| s.trim_matches(|c| c == '(' || c == ')').to_ascii_uppercase())
                    .collect()
            },
        );
    DAEMONS
        .iter()
        .filter(|d| words.iter().any(|w| w == "*ALL" || w == d.keyword))
        .collect()
}

fn active(host: &Host, name: &str) -> bool {
    host.link.borrow_mut().with(|l| l.job_state(name.into())) == 1
}

/// `STRHOSTSVR`.
pub fn start(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let mut out = String::new();
    for d in selected(r) {
        if active(host, d.name) {
            continue;
        }
        let number = host
            .link
            .borrow_mut()
            .with(Client::job_number)
            .map_err(|e| Fail::new("CPF0001", e))?;
        let start = wm::ctl::Start {
            name: d.name.into(),
            user: "QUSER".into(),
            number,
            handle: u64::from(number),
            kind: 'B',
            subsystem: d.subsystem.into(),
            role: "HOSTSVRD".into(),
            jobd: "QSYS/QZBSJOBD".into(),
            class: "QGPL/QCASERVR".into(),
            pool: "*BASE".into(),
            rqsdta: d.keyword.into(),
            ..wm::ctl::Start::default()
        };
        let mut s = crate::work::spawn::job(start).map_err(|e| Fail::new("CPF0001", e))?;
        crate::work::spawn::ready(&mut s).map_err(|e| Fail::new("CPF0001", e))?;
        host.children.borrow_mut().push(s);
        let _ = writeln!(out, "Host server {} started as job {number:06}/QUSER/{}.", d.keyword, d.name);
    }
    if out.is_empty() {
        out.push_str("The host servers named are already active.\n");
    }
    Ok(out)
}

/// `ENDHOSTSVR`.
pub fn end(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let mut out = String::new();
    for d in selected(r) {
        if !active(host, d.name) {
            continue;
        }
        let o = crate::command::run(job, host, &format!("ENDJOB JOB({}) OPTION(*IMMED)", d.name)).text;
        if let Some(id) = super::jobsite::leading_msgid(o.trim()) {
            return Err(Fail::new(id, o.trim().to_string()));
        }
        let _ = writeln!(out, "Host server {} ending.", d.keyword);
    }
    if out.is_empty() {
        out.push_str("The host servers named are not active.\n");
    }
    Ok(out)
}
