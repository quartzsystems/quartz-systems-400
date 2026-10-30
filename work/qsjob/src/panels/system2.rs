// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The second batch of system displays and the editors: `DSPPGM`,
//! `DSPSRVPGM`, `DSPMOD`, `DSPBNDDIR`, `DSPCMD`, `DSPPGMREF`
//! (`programs`), `WRKDEVD`, `WRKHDWRSC`, `DSPHDWRSC`, `DSPPTF`
//! (`config`), `WRKLIND`, `DSPLIND` (`cmn`), `DSPUSRPRF`'s other
//! types, `DSPAUTLOBJ`, `DSPAUDJRNE`
//! (`security`), `EDTLIBL`, `EDTOBJAUT`, `EDTAUTL` and `WRKSYSVAL`'s
//! change (`editors`). A driver returns `None` when the data is not
//! there, so the service's text path answers with its message.

mod cmn;
mod config;
mod editors;
mod programs;
mod security;

pub use editors::change_options;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};
use uim::model::Action;

/// The driver of a command of the batch, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    match r.name.as_str() {
        "DSPPGM" => programs::dsppgm(job, host, surface, r, false),
        "DSPSRVPGM" => programs::dsppgm(job, host, surface, r, true),
        "DSPMOD" => programs::dspmod(host, surface, r),
        "DSPBNDDIR" => programs::dspbnddir(host, surface, r),
        "DSPCMD" => programs::dspcmd(host, surface, r),
        "DSPPGMREF" => Some(programs::dsppgmref(host, surface, r)),
        "WRKDEVD" => config::wrkdevd(job, host, surface, r),
        "WRKHDWRSC" => config::hdwrsc(job, host, surface, r, true),
        "DSPHDWRSC" => config::hdwrsc(job, host, surface, r, false),
        "DSPPTF" => config::dspptf(host, surface),
        "WRKLIND" => cmn::wrklind(job, host, surface, r),
        "DSPLIND" => cmn::dsplind(host, surface, r),
        "DSPUSRPRF" => security::dspusrprf(host, surface, r),
        "DSPAUTLOBJ" => security::dspautlobj(host, surface, r),
        "DSPAUDJRNE" => Some(security::dspaudjrne(host, surface, r)),
        "EDTLIBL" => editors::edtlibl(job, host, surface),
        "EDTOBJAUT" => editors::edtobjaut(job, host, surface, r),
        "EDTAUTL" => editors::edtautl(job, host, surface, r),
        _ => None,
    }
}

/// A parameter's first value as typed or defaulted, upper case.
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

/// A qualified parameter as `(name, library)`, `*LIBL` when no
/// library was given.
fn qual(cmd: &Resolved, kwd: &str) -> (String, String) {
    let up = |v: &Value| v.source().trim().to_ascii_uppercase();
    match cmd.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(up).unwrap_or_default(),
            parts
                .get(1)
                .map(up)
                .filter(|lib| !lib.is_empty())
                .unwrap_or_else(|| "*LIBL".into()),
        ),
        Some(value) => {
            let text = up(value);
            match text.split_once('/') {
                Some((lib, name)) => (name.to_string(), lib.to_string()),
                None => (text, "*LIBL".into()),
            }
        }
        None => (String::new(), "*LIBL".into()),
    }
}

/// The variables and lists of display `what`, `None` when the
/// service has none to give.
fn fetch(host: &Host, what: &str, args: &[&str]) -> Option<stg::proto::Sys2Data> {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    host.link
        .borrow_mut()
        .with(|l| l.sys2_panel(what.into(), args))
        .ok()
}

/// A list's entries from the data, empty when it has none.
fn list_of(data: &stg::proto::Sys2Data, name: &str) -> Vec<Vec<(String, String)>> {
    data.1
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, rows)| rows.clone())
        .unwrap_or_default()
}

/// The data's variables into the dialog.
fn set_vars(d: &mut UimDialog, data: &stg::proto::Sys2Data) {
    for (k, v) in &data.0 {
        d.set(k, v);
    }
}

/// A variable of the data.
fn var_of(data: &stg::proto::Sys2Data, name: &str) -> String {
    data.0
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// Fill a list with entries, every one given; the page kept when it
/// still has entries.
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

/// A list entry's value.
fn cell(d: &UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x.trim().to_string())
        .unwrap_or_default()
}

/// Show a panel, a render failure as the command's escape.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| Fail::new("CPF6A12", format!("panel {panel}: {e}")))
}

/// A dialog over a shipped group, the copyright line shown.
fn dialog(host: &Host, group: &str) -> Result<UimDialog, Fail> {
    let mut d = crate::panels::dialog(host, group)?;
    d.copyright = true;
    Ok(d)
}

/// `---Name---`, `width` wide, the name centered: a heading over a
/// group of columns.
fn dashed(name: &str, width: usize) -> String {
    let left = width.saturating_sub(name.len()) / 2;
    let right = width.saturating_sub(name.len() + left);
    format!("{}{name}{}", "-".repeat(left), "-".repeat(right))
}

/// Run a command from a panel: the line to show after it, an error
/// when it failed. A leading `?` prompts it.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        if f.msgid == "CPF6801" {
            return None;
        }
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        let line = if line.is_empty() {
            format!("{} ended with {}.", out.name, f.msgid)
        } else {
            line
        };
        return Some((line, true));
    }
    out.text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| (l.to_string(), false))
}

/// The options typed on a list, run in order: a `CMD` action with the
/// entry's values, else what `special` makes of it.
fn act(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    choices: &[Choice],
    special: &dyn Fn(&UimDialog, &Choice) -> Option<String>,
) {
    for c in choices {
        let text = if let Some(Action::Cmd(t)) = &c.action {
            d.substitute(t, Some((&c.list, c.row)))
        } else if let Some(t) = special(d, c) {
            t
        } else {
            d.message = Some((format!("Option {} is not valid here.", c.option), true));
            continue;
        };
        if let Some(m) = exec(job, host, &text) {
            d.message = Some(m);
        }
    }
    for c in choices {
        d.list_mut(&c.list).options.clear();
    }
}

/// The text the service answers a command line with, or its message
/// as the escape.
fn service_text(host: &Host, line: &str) -> Result<String, Fail> {
    let text = host.exec(line).text;
    let t = text.trim();
    if let Some(id) = crate::command::jobsite::leading_msgid(t) {
        return Err(Fail::new(id, t.to_string()));
    }
    Ok(text)
}

/// Print is not there yet.
const NO_PRINT: &str = "Print is not available in this release.";
