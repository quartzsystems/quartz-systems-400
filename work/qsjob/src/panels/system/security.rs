// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPAUTL` and `WRKAUTL` (`QUIAUTL`), `DSPSECAUD` (`QUISECAUD`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Next, display, parm, work_answer};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// `DSPAUTL AUTL()`; `OUTPUT(*OUTFILE)` stays with the service.
pub fn dspautl(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    if parm(r, "OUTPUT") == "*OUTFILE" {
        return None;
    }
    let name = parm(r, "AUTL");
    let rows = host
        .link
        .borrow_mut()
        .with(|l| l.autl_panel(name.clone()))
        .ok()?;
    let cell = |r: usize, i: usize| {
        rows.get(r)
            .and_then(|row| row.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let mut d = match crate::panels::dialog(host, "QUIAUTL") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("ALNAME", &name);
    d.set("ALOWNER", &cell(0, 0));
    d.set("ALTEXT", &cell(0, 1));
    for r in 1..rows.len() {
        d.add_row(
            "USERS",
            vec![
                ("ALUSER".into(), cell(r, 0)),
                ("ALAUT".into(), cell(r, 1)),
                ("ALMGT".into(), cell(r, 2)),
            ],
        );
    }
    d.list_mut("USERS").complete = true;
    Some(display(surface, &mut d, "DSPAUTL"))
}

/// `WRKAUTL AUTL()`.
pub fn wrkautl(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let which = parm(r, "AUTL");
    let mut d = match crate::panels::dialog(host, "QUIAUTL") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    loop {
        let lists = host.link.borrow_mut().with(Client::autl_panel_list).ok()?;
        let top = d.list_mut("LISTS").top;
        d.clear_list("LISTS");
        for (name, text) in lists {
            let keep = match which.strip_suffix('*') {
                _ if which.is_empty() || which == "*ALL" => true,
                Some(prefix) => name.starts_with(prefix),
                None => name == which,
            };
            if keep {
                d.add_row(
                    "LISTS",
                    vec![
                        ("OPT".into(), String::new()),
                        ("ALNAME".into(), name),
                        ("ALTEXT".into(), text),
                    ],
                );
            }
        }
        let l = d.list_mut("LISTS");
        l.complete = true;
        l.top = top.min(l.rows.len().saturating_sub(1));
        loop {
            let answer = match d.show(surface, "WRKAUTL") {
                Ok(a) => a,
                Err(e) => return Some(Err(Fail::new("CPF6A12", e.to_string()))),
            };
            match work_answer(job, host, &mut d, "LISTS", answer) {
                Next::Stay => {}
                Next::Refresh => break,
                Next::Leave => return Some(Ok(String::new())),
            }
        }
    }
}

/// `DSPSECAUD`.
pub fn dspsecaud(host: &Host, surface: &mut Surface) -> Option<Result<String, Fail>> {
    let values = host.link.borrow_mut().with(Client::secaud_panel).ok()?;
    let mut d = match crate::panels::dialog(host, "QUISECAUD") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    for (label, v) in &values {
        let var = match label.as_str() {
            "Security journal" => {
                let lib = v.split_once('/').map_or("", |(l, _)| l);
                d.set("SAJRNLIB", lib);
                "SAJRN"
            }
            "Current receiver" => "SARCV",
            "QAUDCTL" => "SACTL",
            "QAUDLVL" => "SALVL",
            "QAUDLVL2" => "SALVL2",
            "QAUDENDACN" => "SAENDACN",
            "QAUDFRCLVL" => "SAFRCLVL",
            "QCRTOBJAUD" => "SACRTAUD",
            _ => continue,
        };
        let shown = if var == "SAJRN" {
            v.split_once('/').map_or(v.as_str(), |(_, n)| n)
        } else {
            v.as_str()
        };
        d.set(var, shown);
    }
    Some(display(surface, &mut d, "DSPSECAUD"))
}
