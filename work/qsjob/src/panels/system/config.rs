// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKCFGSTS` of lines, controllers and devices, and `DSPDEVD`
//! (`QUICFG`). The rows come from the storage service (`cmn_panel`),
//! indented by what hangs off what; F11 switches the job column for
//! the type, F24 the key sets.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{command, display, parm};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

/// A list entry's value.
fn cell(d: &UimDialog, row: usize, var: &str) -> String {
    d.row("CFGS", row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x.trim().to_string())
        .unwrap_or_default()
}

/// What an option that is not a command does with an entry: the
/// command to run, or the message to show.
fn special(d: &UimDialog, row: usize, option: &str) -> Result<String, String> {
    let name = cell(d, row, "CFOBJ");
    let ty = cell(d, row, "CFTYPE");
    match option {
        "5" => {
            let job = cell(d, row, "CFJOB");
            let words: Vec<&str> = job.split_whitespace().collect();
            match words.as_slice() {
                [jobname, user, number] => Ok(format!("WRKJOB JOB({number}/{user}/{jobname})")),
                _ => Err(format!("No job is using {name}.")),
            }
        }
        "8" => match ty.as_str() {
            "*LIN" => Ok(format!("WRKLIND LIND({name})")),
            "*DEV" => Ok(format!("WRKDEVD DEVD({name})")),
            _ => Err(format!(
                "Controller {name} has no description display in this release."
            )),
        },
        "9" | "13" => Err(format!("{name} has no modes or APPN status.")),
        o => Err(format!("Option {o} is not valid.")),
    }
}

/// `WRKCFGSTS CFGTYPE(*LIN|*CTL|*DEV) CFGD() RANGE() STATUS()`.
#[allow(clippy::too_many_lines)] // the keys of the display
pub fn wrkcfgsts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let ty = parm(r, "CFGTYPE");
    if !matches!(ty.as_str(), "*LIN" | "*CTL" | "*DEV") {
        return None;
    }
    let args = vec![
        ty.clone(),
        parm(r, "CFGD"),
        parm(r, "RANGE"),
        parm(r, "STATUS"),
    ];
    let mut d = match crate::panels::dialog(host, "QUICFG") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.copyright = false;
    let mut last = String::new();
    loop {
        let data = host
            .link
            .borrow_mut()
            .with(|l| l.cmn_panel("CFGSTS".into(), args.clone()));
        let data = match data {
            Ok(x) => x,
            Err(e) => {
                let id = crate::command::jobsite::leading_msgid(&e)
                    .unwrap_or("CPF9898")
                    .to_string();
                return Some(Err(Fail::new(&id, e)));
            }
        };
        let rows = data
            .1
            .into_iter()
            .find(|(n, _)| n == "CFGS")
            .map(|(_, r)| r)
            .unwrap_or_default();
        let top = d.list_mut("CFGS").top;
        d.clear_list("CFGS");
        let count = rows.len();
        for row in rows {
            d.add_row("CFGS", row);
        }
        let entries = d.list_mut("CFGS");
        entries.complete = true;
        entries.top = if top < count { top } else { 0 };
        d.set("*DATETIME", &crate::panels::work::stamp(host));
        loop {
            let resp = match d.show(surface, "WRKCFGSTS") {
                Ok(a) => a,
                Err(e) => return Some(Err(Fail::new("CPF6A12", e.to_string()))),
            };
            match resp {
                Response::Exit
                | Response::Cancel
                | Response::Gone
                | Response::Menu(_)
                | Response::Panel(_) => {
                    return Some(Ok(String::new()));
                }
                Response::Options(choices) => {
                    for c in &choices {
                        let text = match &c.action {
                            Some(uim::model::Action::Cmd(t)) => {
                                Ok(d.substitute(t, Some((&c.list, c.row))))
                            }
                            _ => special(&d, c.row, c.option.trim()),
                        };
                        d.message = match text {
                            Ok(t) => command(job, host, &t),
                            Err(m) => Some((m, true)),
                        };
                    }
                    d.list_mut("CFGS").options.clear();
                    break;
                }
                Response::Command(text) => {
                    last = text.trim_start_matches('?').to_string();
                    let text = d.substitute(&text, None);
                    d.message = command(job, host, &text);
                    d.set(CMDLINE_VAR, "");
                    break;
                }
                Response::Retrieve => d.set(CMDLINE_VAR, &last),
                Response::Return(11) => d.view = 1 - d.view.min(1),
                Response::Return(24) => {
                    let next = if d.get("KEYSET") == "2" { "1" } else { "2" };
                    d.set("KEYSET", next);
                }
                Response::Return(23) => {
                    d.message =
                        Some(("There are no more options for these objects.".into(), false));
                }
                Response::Return(14) => {
                    d.message = command(job, host, "WRKLIND");
                    break;
                }
                Response::Return(4) => {
                    d.message =
                        Some(("Type a command to prompt on the command line.".into(), true));
                }
                Response::Return(_) => {
                    // Enter, F5: position the list and look again.
                    let pos = d.get("POSTO").trim().to_ascii_uppercase();
                    if !pos.is_empty() {
                        let at = (0..count).find(|&i| cell(&d, i, "CFOBJ") >= pos);
                        d.list_mut("CFGS").top = at.unwrap_or(0);
                        d.set("POSTO", "");
                    }
                    break;
                }
                Response::Print => {
                    d.message = Some(("Print is not available in this release.".into(), true));
                }
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
