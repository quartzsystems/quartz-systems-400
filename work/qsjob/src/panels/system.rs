// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System and security displays: `DSPSYSVAL`/`WRKSYSVAL`,
//! `DSPSFWRSC`, `DSPNETA`, `DSPAUTUSR`, `DSPUSRPRF`/`WRKUSRPRF`,
//! `WRKCFGSTS`, `DSPDEVD`, `DSPAUTL`/`WRKAUTL`, `DSPSECAUD`,
//! `WRKREGINF`.
//! `TODO(phase8)`: `DSPSGNINF` (no shipped definition yet). A driver
//! returns `None` when the data is not there (no such value or
//! profile), so the service's text path answers with its message.

mod config;
mod exits;
mod netattr;
mod security;
mod sysval;
mod usrprf;

use cmd::{Resolved, Value};
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

/// The driver of a system or security command, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    match r.name.as_str() {
        "DSPSYSVAL" => sysval::dspsysval(host, surface, r),
        "WRKSYSVAL" => sysval::wrksysval(job, host, surface, r),
        "DSPNETA" => netattr::dspneta(host, surface),
        "DSPSFWRSC" => netattr::dspsfwrsc(host, surface),
        "DSPUSRPRF" => usrprf::dspusrprf(host, surface, r),
        "WRKUSRPRF" => usrprf::wrkusrprf(job, host, surface, r),
        "DSPAUTUSR" => usrprf::dspautusr(host, surface, r),
        "WRKCFGSTS" => config::wrkcfgsts(job, host, surface, r),
        "DSPDEVD" => config::dspdevd(host, surface, r),
        "DSPAUTL" => security::dspautl(host, surface, r),
        "WRKAUTL" => security::wrkautl(job, host, surface, r),
        "DSPSECAUD" => security::dspsecaud(host, surface),
        "WRKREGINF" => exits::wrkreginf(job, host, surface, r),
        _ => None,
    }
}

/// A parameter's value as typed or defaulted, upper case.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim_matches('\'')
        .trim()
        .to_ascii_uppercase()
}

/// Show a display panel until the operator leaves it.
fn display(surface: &mut Surface, d: &mut UimDialog, panel: &str) -> Result<String, Fail> {
    loop {
        match d.show(surface, panel) {
            Ok(Response::Print) => {
                d.message = Some(("Print is not available in this release.".into(), true));
            }
            Ok(
                Response::Return(_)
                | Response::Exit
                | Response::Cancel
                | Response::Gone
                | Response::Menu(_)
                | Response::Panel(_),
            ) => return Ok(String::new()),
            Ok(_) => {}
            Err(e) => return Err(crate::panels::failed(e.to_string())),
        }
    }
}

/// Run a command from a list option or the command line; the message
/// to show after it, if any.
fn command(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    let o = crate::command::run(job, host, text);
    if let Some(f) = &o.escape {
        if f.msgid == "CPF6801" {
            return None;
        }
        let t = f.text.lines().last().unwrap_or("").trim();
        return Some((
            if t.is_empty() { f.msgid.clone() } else { t.to_string() },
            true,
        ));
    }
    o.text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| (l.to_string(), false))
}

/// What a work-with list does with an answer: go on (after the options
/// ran or a command), refresh, or leave.
enum Next {
    Stay,
    Refresh,
    Leave,
}

/// The common answers of a work-with panel: options run their
/// commands, the command line runs, F5 refreshes, F3 and F12 leave.
fn work_answer(job: &mut Job, host: &Host, d: &mut UimDialog, list: &str, r: Response) -> Next {
    match r {
        Response::Options(choices) => {
            run_choices(job, host, d, list, &choices);
            Next::Refresh
        }
        Response::Command(text) => {
            let text = d.substitute(&text, None);
            d.message = command(job, host, &text);
            d.set(crate::display::uim::CMDLINE_VAR, "");
            Next::Refresh
        }
        Response::Return(5) => Next::Refresh,
        Response::Print => {
            d.message = Some(("Print is not available in this release.".into(), true));
            Next::Stay
        }
        Response::Retrieve | Response::Return(_) => Next::Stay,
        Response::Exit
        | Response::Cancel
        | Response::Gone
        | Response::Menu(_)
        | Response::Panel(_) => Next::Leave,
    }
}

/// Each option typed on an entry, its command run with the entry's
/// variables.
fn run_choices(job: &mut Job, host: &Host, d: &mut UimDialog, list: &str, choices: &[Choice]) {
    let mut shown = None;
    for c in choices {
        match &c.action {
            Some(uim::model::Action::Cmd(text)) => {
                let text = d.substitute(text, Some((&c.list, c.row)));
                if let Some(m) = command(job, host, &text) {
                    shown = Some(m);
                }
            }
            _ => {
                shown = Some((format!("Option {} is not valid.", c.option.trim()), true));
            }
        }
    }
    d.list_mut(list).options.clear();
    d.message = shown;
}
