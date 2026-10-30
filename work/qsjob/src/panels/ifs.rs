// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS displays (`QUIWRKLNK`, `QUIWRKAUT`): `WRKLNK` and `DSPLNK`
//! with their options (`links`), an object's attributes (`attrs`),
//! `WRKAUT` and `DSPAUT` (`auth`). The data comes through the job's
//! link to the storage service's IFS (`stg::remote::RemoteIfs`) and
//! `ifscmd::display`; options run commands as typed on Command Entry.

mod attrs;
mod auth;
mod links;

use cmd::{Resolved, Value};
use ifs::Errno;
use mi::Job;
use stg::remote::RemoteIfs;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// The driver of an IFS display, if the command has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    // OUTPUT(*PRINT) is the service's text form.
    let printed = matches!(text(r, "OUTPUT").as_str(), "P" | "*PRINT");
    Some(match r.name.as_str() {
        "WRKLNK" => links::run(job, host, surface, r, true),
        "DSPLNK" if !printed => links::run(job, host, surface, r, false),
        "WRKAUT" => auth::run(job, host, surface, &text(r, "OBJ"), true),
        "DSPAUT" if !printed => auth::run(job, host, surface, &text(r, "OBJ"), false),
        _ => return None,
    })
}

/// A parameter's text as given: paths keep their case.
fn text(r: &Resolved, kwd: &str) -> String {
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Char(s)) => s.trim_end().to_string(),
        Some(v) => v.source().trim().trim_matches('\'').to_string(),
        None => String::new(),
    }
}

/// The IFS over the job's link.
fn client(host: &Host) -> RemoteIfs {
    RemoteIfs::new(host.link.clone())
}

/// The escape a display ends with when its object is not there.
fn fail(e: Errno, path: &str) -> Fail {
    let m = ifscmd::Msg::errno(e, path);
    Fail {
        msgid: m.id,
        data: m.data,
        text: format!("{path}: {}", e.text()),
    }
}

/// A panel shown, its failure an escape.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| Fail::new("CPF6A12", format!("panel {panel}: {e}")))
}

/// Run a command as Command Entry would; its first line or its escape
/// goes on the message line.
fn exec(job: &mut Job, host: &Host, d: &mut UimDialog, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        let line = out
            .text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map_or_else(|| format!("{} {}", f.msgid, f.text), str::to_string);
        d.message = Some((line, true));
    } else if let Some(l) = out.text.lines().map(str::trim).find(|l| !l.is_empty()) {
        d.message = Some((l.to_string(), false));
    }
}

/// A row's value.
fn cell(d: &UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x)
        .unwrap_or_default()
}

/// A row of `(variable, value)` pairs.
fn row(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

/// Rows into a list, the page kept where it still fits.
fn fill(d: &mut UimDialog, list: &str, rows: Vec<Vec<(String, String)>>) {
    let n = rows.len();
    let l = d.list_mut(list);
    let top = l.top;
    l.rows = rows;
    l.complete = true;
    l.options.clear();
    l.errors.clear();
    l.top = if top < n { top } else { 0 };
}
