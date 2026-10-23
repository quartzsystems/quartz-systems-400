// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The configuration displays: `WRKDEVD`, `WRKHDWRSC`/`DSPHDWRSC` and
//! `DSPPTF`.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{NO_PRINT, act, cell, dialog, exec, fetch, fill, list_of, parm, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response};

/// Whether a device name passes `DEVD`: `*ALL`, a generic `AB*`, a
/// name, or a special value (every description).
fn wanted(which: &str, name: &str) -> bool {
    if which.is_empty() || which.starts_with('*') {
        return true;
    }
    match which.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == which,
    }
}

/// `WRKDEVD`: the device descriptions with their options; `Position
/// to` starts the list at a name.
pub fn wrkdevd(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let which = parm(r, "DEVD");
    let mut data = fetch(host, "DEVD", &[])?;
    let mut d = match dialog(host, "QUIWRKDEVD") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let mut last = String::new();
    loop {
        let rows: Vec<_> = list_of(&data, "DEVS")
            .into_iter()
            .filter(|row| {
                row.iter()
                    .any(|(k, v)| k == "DEVD" && wanted(&which, v))
            })
            .collect();
        fill(&mut d, "DEVS", rows);
        let resp = match show(&mut d, surface, "WRKDEVD") {
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
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(6) => d.message = exec(job, host, "?CRTDEVDSP"),
            Response::Return(14) => d.message = exec(job, host, "WRKCFGSTS CFGTYPE(*DEV)"),
            Response::Return(4) => {
                d.message = Some(("Type a command to prompt on the command line.".into(), true));
            }
            Response::Return(_) => {
                // Enter: position the list; F5 and Enter both look again.
                let pos = d.get("POSTO").trim().to_ascii_uppercase();
                if let Some(fresh) = fetch(host, "DEVD", &[]) {
                    data = fresh;
                }
                if !pos.is_empty() {
                    let l = d.list_mut("DEVS");
                    let at = l
                        .rows
                        .iter()
                        .position(|row| row.iter().any(|(k, v)| k == "DEVD" && *v >= pos));
                    l.top = at.unwrap_or(0);
                    d.set("POSTO", "");
                }
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// `WRKHDWRSC` and `DSPHDWRSC`: the resources of the `TYPE` asked for.
pub fn hdwrsc(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    work: bool,
) -> Option<Result<String, Fail>> {
    let ty = parm(r, "TYPE");
    let mut d = match dialog(host, "QUIHDWRSC") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let panel = if work { "WRKHDWRSC" } else { "DSPHDWRSC" };
    loop {
        let data = fetch(host, "HDWRSC", &[&ty])?;
        super::set_vars(&mut d, &data);
        fill(&mut d, "RSCS", list_of(&data, "RSCS"));
        let resp = match show(&mut d, surface, panel) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Some(Ok(String::new())),
            Response::Return(0) if !work => return Some(Ok(String::new())),
            Response::Options(c) => {
                for ch in &c {
                    let name = cell(&d, &ch.list, ch.row, "RSCNAME");
                    let text = cell(&d, &ch.list, ch.row, "RSCTEXT");
                    d.message = match ch.option.as_str() {
                        "5" => exec(job, host, "WRKCFGSTS CFGTYPE(*DEV)"),
                        _ => Some((format!("{name}: {text}."), false)),
                    };
                }
                d.list_mut("RSCS").options.clear();
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// `DSPPTF`: the product and its fixes (none in this release).
pub fn dspptf(host: &Host, surface: &mut Surface) -> Option<Result<String, Fail>> {
    let rows = host.link.borrow_mut().with(Client::sfwrsc_panel);
    let first = rows.first()?;
    let mut d = match dialog(host, "QUIDSPPTF") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("PRODID", first.first().map_or("", String::as_str));
    d.set("RELEASE", first.get(6).map_or("", String::as_str));
    fill(&mut d, "PTFS", Vec::new());
    loop {
        match show(&mut d, surface, "DSPPTF") {
            Ok(Response::Exit | Response::Cancel | Response::Gone | Response::Return(0)) => {
                return Some(Ok(String::new()));
            }
            Ok(Response::Return(11)) => {
                d.message = Some(("There are no PTFs to show another view of.".into(), false));
            }
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }
    }
}
