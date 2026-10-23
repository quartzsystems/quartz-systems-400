// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPUSRPRF TYPE(*BASIC)` and `WRKUSRPRF` (`QUIUSRPRF`), `DSPAUTUSR`
//! (`QUIAUTUSR`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Next, display, parm, work_answer};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `DSPUSRPRF USRPRF() TYPE(*BASIC)`; the other types stay text.
pub fn dspusrprf(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let ty = parm(r, "TYPE");
    if !(ty.is_empty() || ty == "*BASIC") {
        return None;
    }
    let name = parm(r, "USRPRF");
    let vars = host
        .link
        .borrow_mut()
        .with(|l| l.usrprf_panel(name.clone()))
        .ok()?;
    let mut d = match crate::panels::dialog(host, "QUIUSRPRF") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    for (k, v) in &vars {
        d.set(k, v);
    }
    Some(display(surface, &mut d, "DSPUSRPRF"))
}

/// Whether `name` matches `*ALL`, a generic `ABC*` or a name.
fn matches(which: &str, name: &str) -> bool {
    if which.is_empty() || which == "*ALL" {
        return true;
    }
    match which.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == which,
    }
}

/// `WRKUSRPRF USRPRF()`.
pub fn wrkusrprf(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let which = parm(r, "USRPRF");
    let mut d = match crate::panels::dialog(host, "QUIUSRPRF") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    loop {
        let users = host.link.borrow_mut().with(Client::usrprf_panel_list).ok()?;
        let top = d.list_mut("USERS").top;
        d.clear_list("USERS");
        for (name, class, text) in users {
            if matches(&which, &name) {
                d.add_row(
                    "USERS",
                    vec![
                        ("OPT".into(), String::new()),
                        ("WUPNAME".into(), name),
                        ("WUPCLS".into(), class),
                        ("WUPTEXT".into(), text),
                    ],
                );
            }
        }
        let l = d.list_mut("USERS");
        l.complete = true;
        l.top = top.min(l.rows.len().saturating_sub(1));
        loop {
            let answer = match d.show(surface, "WRKUSRPRF") {
                Ok(a) => a,
                Err(e) => return Some(Err(Fail::new("CPF6A12", e.to_string()))),
            };
            match work_answer(job, host, &mut d, "USERS", answer) {
                Next::Stay => {}
                Next::Refresh => break,
                Next::Leave => return Some(Ok(String::new())),
            }
        }
    }
}

/// `DSPAUTUSR SEQ()`.
pub fn dspautusr(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let by_group = parm(r, "SEQ") == "*GRPPRF";
    let rows = host
        .link
        .borrow_mut()
        .with(|l| l.autusr_panel(by_group))
        .ok()?;
    let mut d = match crate::panels::dialog(host, "QUIAUTUSR") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = true;
    for row in rows {
        let get = |i: usize| row.get(i).cloned().unwrap_or_default();
        d.add_row(
            "USERS",
            vec![
                ("AUUSER".into(), get(0)),
                ("AUGROUP".into(), get(1)),
                ("AUPWDCHG".into(), get(2)),
                ("AUNOPWD".into(), get(3)),
                ("AUTEXT".into(), get(4)),
            ],
        );
    }
    d.list_mut("USERS").complete = true;
    loop {
        match d.show(surface, "DSPAUTUSR") {
            Ok(Response::Return(11)) => {
                d.message = Some((
                    "Every profile here keeps one password, for every password level.".into(),
                    false,
                ));
            }
            Ok(Response::Print) => {
                d.message = Some(("Print is not available in this release.".into(), true));
            }
            Ok(Response::Options(_) | Response::Command(_) | Response::Retrieve) => {}
            Ok(_) => return Some(Ok(String::new())),
            Err(e) => return Some(Err(Fail::new("CPF6A12", e.to_string()))),
        }
        d.copyright = false;
    }
}
