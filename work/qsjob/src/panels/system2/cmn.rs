// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The line descriptions (`QUILIND`): `WRKLIND` with its options and
//! F11 view, `DSPLIND` with its values after their keywords until F11
//! hides them. The data comes from the storage service (`cmn_panel`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{NO_PRINT, act, dialog, exec, fill, parm, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response};

/// The service's data for `what`, or its refusal as the escape.
fn fetch(host: &Host, what: &str, args: &[String]) -> Result<stg::proto::Sys2Data, Fail> {
    let r = host
        .link
        .borrow_mut()
        .with(|l| l.cmn_panel(what.into(), args.to_vec()));
    r.map_err(|e| {
        let id = crate::command::jobsite::leading_msgid(&e)
            .unwrap_or("CPF9898")
            .to_string();
        Fail::new(&id, e)
    })
}

/// `WRKLIND LIND()`.
pub fn wrklind(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let which = parm(r, "LIND");
    let mut d = match dialog(host, "QUILIND") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let mut last = String::new();
    loop {
        let data = match fetch(host, "LIND", std::slice::from_ref(&which)) {
            Ok(x) => x,
            Err(f) => return Some(Err(f)),
        };
        let rows = data
            .1
            .into_iter()
            .find(|(n, _)| n == "LINES")
            .map(|(_, r)| r)
            .unwrap_or_default();
        fill(&mut d, "LINES", rows);
        let resp = match show(&mut d, surface, "WRKLIND") {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Some(Ok(String::new())),
            Response::Options(c) => {
                act(job, host, &mut d, &c, &|_, c| {
                    (c.option == "9").then(String::new)
                });
                if c.iter().any(|c| c.option == "9") {
                    d.message = Some((
                        "Retrieving configuration source is not available in this release.".into(),
                        true,
                    ));
                }
            }
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(6) => d.message = exec(job, host, "?CRTLINETH"),
            Response::Return(11) => d.view = 1 - d.view.min(1),
            Response::Return(14) => d.message = exec(job, host, "WRKCFGSTS CFGTYPE(*LIN)"),
            Response::Return(4) => {
                d.message = Some(("Type a command to prompt on the command line.".into(), true));
            }
            Response::Return(_) => {
                let pos = d.get("POSTO").trim().to_ascii_uppercase();
                if !pos.is_empty() {
                    let l = d.list_mut("LINES");
                    let at = l
                        .rows
                        .iter()
                        .position(|row| row.iter().any(|(k, v)| k == "LIND" && *v >= pos));
                    l.top = at.unwrap_or(0);
                    d.set("POSTO", "");
                }
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// `DSPLIND LIND()`.
pub fn dsplind(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    if parm(r, "OUTPUT") == "*PRINT" {
        return None;
    }
    let name = parm(r, "LIND");
    let data = match fetch(host, "DSPLIND", std::slice::from_ref(&name)) {
        Ok(x) => x,
        Err(f) => return Some(Err(f)),
    };
    let mut d = match dialog(host, "QUILIND") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let agg = data.0.iter().any(|(k, v)| k == "AGG" && v == "1");
    let panel = if agg { "DSPLINDA" } else { "DSPLIND" };
    let mut keywords = true;
    loop {
        for (k, v) in &data.0 {
            // A value carries its keyword in its first twelve columns.
            let shown = if keywords {
                v.clone()
            } else {
                v.chars().skip(12).collect()
            };
            d.set(k, &shown);
        }
        d.set("*DATETIME", &crate::panels::work::stamp(host));
        match show(&mut d, surface, panel) {
            Ok(Response::Return(11)) => keywords = !keywords,
            Ok(Response::Print) => d.message = Some((NO_PRINT.into(), true)),
            Ok(_) => return Some(Ok(String::new())),
            Err(e) => return Some(Err(e)),
        }
    }
}
