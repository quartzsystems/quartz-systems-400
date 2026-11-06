// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Time zone descriptions (`QSTIMZON`): `WRKTIMZON`, its four views
//! (F11 goes round them), options 2 (`CHGTIMZON`), 4 (`DLTTIMZON`), 5
//! (Display Time Zone Description Details), 6 (printed) and 8
//! (`CHGSYSVAL QTIMZON`), and F6 (`CRTTIMZON`).

use cmd::Resolved;
use mi::Job;

use super::{Step, cell, choice, dialog, display, exec, fetch, load, no_more, parm, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{LineMsg, Response, UimDialog};

/// The views F11 goes round.
const VIEWS: [&str; 4] = ["WRKTIMZON", "WRKTIMZON2", "WRKTIMZON3", "WRKTIMZON4"];

/// `WRKTIMZON TIMZON(name|generic*|*ALL)`.
pub fn wrktimzon(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let filter = parm(r, "TIMZON").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QSTIMZON")?;
    let mut view = 0usize;
    loop {
        let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
            d.set("*DATETIME", &crate::panels::work::stamp(host));
            load(d, "ZONES", fetch(host, "TIMZON", &[&filter])?);
            Ok(())
        };
        let mut next = None;
        let mut keys = |job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(11) => {
                    next = Some((view + 1) % VIEWS.len());
                    Step::Leave
                }
                Response::Return(6) => {
                    exec(job, host, d, "?CRTTIMZON");
                    Step::Reload
                }
                Response::Return(1) => Step::Reload,
                Response::Options(ref c) => match choice(&resp).map(|c| c.option.as_str()) {
                    Some("5") => {
                        let name = cell(d, &c[0].list, c[0].row, "TIMZON");
                        details(host, surface, name.trim())?;
                        Step::Reload
                    }
                    Some("6") => {
                        let name = cell(d, &c[0].list, c[0].row, "TIMZON");
                        match fetch(host, "TIMZONPRT", &[name.trim()]) {
                            Ok((vars, _)) => {
                                if let Some((_, t)) = vars.iter().find(|(k, _)| k == "DONE") {
                                    d.send(LineMsg { text: Some(t.clone()), ..LineMsg::default() });
                                }
                            }
                            Err(f) => d.send(LineMsg { id: f.msgid, error: true, text: Some(f.text), ..LineMsg::default() }),
                        }
                        Step::Stay
                    }
                    _ => no_more(d, &resp),
                },
                other => no_more(d, &other),
            })
        };
        work(job, host, surface, &mut d, VIEWS[view], &mut reload, &mut keys)?;
        match next {
            Some(v) => view = v,
            None => return Ok(String::new()),
        }
    }
}

/// Option 5: the details display of one description.
fn details(host: &Host, surface: &mut Surface, name: &str) -> Result<(), Fail> {
    let mut d = dialog(host, "QSTIMZON")?;
    let data = fetch(host, "TIMZONDTL", &[name])?;
    let panel = data.0.iter().find(|(k, _)| k == "PANEL").map_or_else(|| "DSPTZN".to_string(), |(_, v)| v.clone());
    let mut reload = |_: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        for (k, v) in &data.0 {
            d.set(k, v);
        }
        Ok(())
    };
    display(host, surface, &mut d, &panel, &mut reload)?;
    Ok(())
}
