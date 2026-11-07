// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays that had only a text listing before the CPP audit
//! (`docs/CPP_AUDIT.md`): the PTF displays (`ptf`), `DSPIPLA`,
//! `WRKMSGD`, `WRKRDBDIRE` and `WRKDDMF` (`lists`), and the eight that
//! showed the report viewer before: `DSPPTFCVR` (`ptf`), `DSPBCKSTS`
//! (`bcksts`), `DSPLICKEY` (`keys`), `DSPTAP`, `DSPTAPCTG`, `DSPOPT` and
//! `WRKOPTVOL` (`media`), and `DSPUDFS` (`udfs`).

mod bcksts;
mod fcnusg;
mod keys;
mod lists;
mod media;
mod ptf;
mod udfs;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{LineMsg, Response, UimDialog};
use uim::model::Action;

/// The driver of a command of the batch, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "DSPPTF" => ptf::dspptf(job, host, surface, r),
        "WRKPTFGRP" => ptf::wrkptfgrp(host, surface, r),
        "DSPIPLA" => lists::dspipla(host, surface),
        "WRKMSGD" => lists::wrkmsgd(job, host, surface, r),
        "WRKRDBDIRE" => lists::wrkrdbdire(job, host, surface, r),
        "WRKDDMF" => lists::wrkddmf(job, host, surface, r),
        "DSPPTFCVR" => ptf::dspptfcvr(job, host, surface, r),
        "DSPLICKEY" => keys::dsplickey(job, host, surface, r),
        "DSPBCKSTS" => bcksts::dspbcksts(host, surface),
        "DSPTAP" => media::dsptap(host, surface, r),
        "DSPTAPCTG" => media::dsptapctg(host, surface, r),
        "DSPOPT" => media::dspopt(host, surface, r),
        "WRKOPTVOL" => media::wrkoptvol(job, host, surface, r),
        "DSPUDFS" => udfs::dspudfs(host, surface, r),
        "DSPSECA" if !printed(r) => fcnusg::dspseca(host, surface),
        "DSPFCNUSG" if !printed(r) => fcnusg::dspfcnusg(host, surface, r),
        "WRKFCNUSG" => fcnusg::wrkfcnusg(job, host, surface, r),
        _ => return None,
    })
}

/// `OUTPUT(*PRINT)`: the service prints it.
fn printed(r: &Resolved) -> bool {
    parm(r, "OUTPUT") == "*PRINT"
}

/// A parameter's first value, upper case, quotes off.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::List(items) => items.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        })
        .unwrap_or_default()
        .trim()
        .trim_matches('\'')
        .trim()
        .to_ascii_uppercase()
}

/// Every value of a list parameter, upper case.
fn parm_list(r: &Resolved, kwd: &str) -> Vec<String> {
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| v.source().trim().to_ascii_uppercase())
            .collect(),
        Some(v) => vec![v.source().trim().to_ascii_uppercase()],
        None => Vec::new(),
    }
}

/// A qualified parameter as `(name, library)`, `*LIBL` when no library
/// was given.
fn qual(r: &Resolved, kwd: &str) -> (String, String) {
    let up = |v: &Value| v.source().trim().to_ascii_uppercase();
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(up).unwrap_or_default(),
            parts
                .get(1)
                .map(up)
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| "*LIBL".into()),
        ),
        Some(v) => (up(v), "*LIBL".into()),
        None => (String::new(), "*LIBL".into()),
    }
}

/// The service's data of display `what`; its failure is the
/// command's escape.
fn fetch(host: &Host, what: &str, args: &[String]) -> Result<stg::proto::Sys2Data, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.sys2_panel(what.into(), args.to_vec()))
        .map_err(|e| crate::panels::failed(&e))
}

/// The data's variables into the dialog, its lists filled.
fn load(d: &mut UimDialog, data: stg::proto::Sys2Data) {
    for (k, v) in &data.0 {
        d.set(k, v);
    }
    for (name, rows) in data.1 {
        fill(d, &name, rows);
    }
}

/// Fill a list, the page kept when it still has entries.
fn fill(d: &mut UimDialog, list: &str, rows: Vec<Vec<(String, String)>>) {
    let top = d.list_mut(list).top;
    d.clear_list(list);
    let n = rows.len();
    for r in rows {
        d.add_row(list, r);
    }
    let l = d.list_mut(list);
    l.complete = true;
    l.top = if top < n { top } else { 0 };
}

/// A dialog over a shipped group.
fn dialog(host: &Host, group: &str) -> Result<UimDialog, Fail> {
    crate::panels::dialog(host, group)
}

/// Show a panel; a render failure is the function check.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| crate::panels::failed(format!("panel {panel}: {e}")))
}

/// Run a command from a panel: the line to show after it.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<LineMsg> {
    crate::panels::exec(job, host, text)
}

/// The `CMD` options typed on a list, run in order; `special` answers
/// the others (its message, or `None` to skip).
fn act(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    choices: &[crate::display::uim::Choice],
    special: &mut dyn FnMut(&mut UimDialog, &crate::display::uim::Choice) -> Result<(), Fail>,
) -> Result<(), Fail> {
    for c in choices {
        if let Some(Action::Cmd(t)) = &c.action {
            let text = crate::panels::option_command(d, c, t);
            if let Some(m) = exec(job, host, &text) {
                d.send(m);
            }
        } else {
            special(d, c)?;
        }
    }
    for c in choices {
        d.list_mut(&c.list).options.clear();
    }
    Ok(())
}

/// The date and time of a `TOPSEP=DATETIME` display these commands
/// show without the zone, as IBM's do.
fn datetime(host: &Host) -> String {
    let s = crate::panels::work::stamp(host);
    let mut w = s.split_whitespace();
    match (w.next(), w.next()) {
        (Some(d), Some(t)) => format!("{d}  {t}"),
        _ => s,
    }
}
