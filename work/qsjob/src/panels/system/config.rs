// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKCFGSTS CFGTYPE(*DEV)` and `DSPDEVD` (`QUICFG`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Next, display, parm, work_answer};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// `WRKCFGSTS CFGTYPE(*DEV) CFGD()`; other types stay text.
pub fn wrkcfgsts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    if parm(r, "CFGTYPE") != "*DEV" {
        return None;
    }
    let which = parm(r, "CFGD");
    let mut d = match crate::panels::dialog(host, "QUICFG") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    loop {
        let devices = host.link.borrow_mut().with(Client::cfgsts_panel).ok()?;
        let top = d.list_mut("DEVICES").top;
        d.clear_list("DEVICES");
        for dev in devices {
            let get = |i: usize| dev.get(i).cloned().unwrap_or_default();
            let name = get(0);
            let keep = match which.strip_suffix('*') {
                _ if which.is_empty() || which == "*ALL" => true,
                Some(prefix) => name.starts_with(prefix),
                None => name == which,
            };
            if keep {
                d.add_row(
                    "DEVICES",
                    vec![
                        ("OPT".into(), String::new()),
                        ("CFNAME".into(), name),
                        ("CFSTS".into(), get(1)),
                        ("CFTEXT".into(), get(2)),
                    ],
                );
            }
        }
        let l = d.list_mut("DEVICES");
        l.complete = true;
        l.top = top.min(l.rows.len().saturating_sub(1));
        loop {
            let answer = match d.show(surface, "WRKCFGSTS") {
                Ok(a) => a,
                Err(e) => return Some(Err(Fail::new("CPF6A12", e.to_string()))),
            };
            match work_answer(job, host, &mut d, "DEVICES", answer) {
                Next::Stay => {}
                Next::Refresh => break,
                Next::Leave => return Some(Ok(String::new())),
            }
        }
    }
}

/// `DSPDEVD DEVD()`.
pub fn dspdevd(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let name = parm(r, "DEVD");
    let vars = host
        .link
        .borrow_mut()
        .with(|l| l.devd_panel(name.clone()))
        .ok()?;
    let mut d = match crate::panels::dialog(host, "QUICFG") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    for (k, v) in &vars {
        d.set(k, v);
    }
    Some(display(surface, &mut d, "DSPDEVD"))
}
