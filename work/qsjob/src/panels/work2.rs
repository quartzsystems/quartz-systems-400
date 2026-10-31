// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panels batch 2: work management, spool, files and journals.
//! `WRKSBMJOB`, `WRKSBSD` (`jobs`); `WRKSYSSTS`, `WRKDSKSTS`,
//! `WRKSYSACT`, `WRKSHRPOOL` (`sys`); `WRKOUTQD`, `DSPSPLF` (`spool`);
//! `WRKLIB`, `WRKOBJLCK`, `WRKOBJPGP`, `DSPOVR`, `DSPSAVF`, `WRKPRB`
//! (`objs`); `DSPPFM` (`pfm`); `WRKJRN`, `WRKJRNA`, `DSPJRN`,
//! `DSPJRNRCVA` (`jrn`); the service displays `WRKPRB`, `DSPPRB`, `ANZPRB`,
//! `WRKCNTINF`, `WRKSRVPVD`, `DSPSRVSTS` (`service`); `WRKIMGCLG`,
//! `WRKIMGCLGE`, `WRKTAPCTG`, `WRKMLBSTS` (`media`). Their data comes through `panel2_data`, named
//! as the panel groups' variables; `viewer` is the window the two
//! viewers share.

// Throughout: `d` the dialog, `r` the command, `c` a choice.
#![allow(clippy::many_single_char_names)]

mod jobs;
mod jrn;
mod media;
mod objs;
mod pfm;
mod service;
mod spool;
mod sys;
mod viewer;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;
use uim::model::Action;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog};

/// The driver of a batch 2 command, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "WRKSBMJOB" => jobs::wrksbmjob(job, host, surface),
        "WRKSBSD" => jobs::wrksbsd(job, host, surface, r),
        "WRKSYSSTS" => sys::wrksyssts(job, host, surface, r),
        "WRKDSKSTS" => sys::wrkdsksts(job, host, surface, r),
        "WRKSYSACT" => sys::wrksysact(job, host, surface),
        "WRKSHRPOOL" => sys::wrkshrpool(job, host, surface),
        "WRKOUTQD" => spool::wrkoutqd(job, host, surface, r),
        "DSPSPLF" => spool::dspsplf(host, surface, r),
        "WRKLIB" => objs::wrklib(job, host, surface, r),
        "WRKOBJLCK" => objs::wrkobjlck(job, host, surface, r),
        "WRKOBJPGP" => objs::wrkobjpgp(job, host, surface, r),
        "DSPOVR" => objs::dspovr(host, surface),
        "DSPSAVF" => objs::dspsavf(host, surface, r),
        "WRKPRB" | "DSPPRB" => service::wrkprb(job, host, surface, r),
        "WRKIMGCLG" => media::wrkimgclg(job, host, surface, r),
        "WRKIMGCLGE" => media::wrkimgclge(job, host, surface, &parm(r, "IMGCLG").unwrap_or_default()),
        "WRKTAPCTG" => media::wrktapctg(job, host, surface, r),
        "WRKMLBSTS" => media::wrkmlbsts(job, host, surface, r),
        "ANZPRB" => service::anzprb(host, surface),
        "WRKCNTINF" => service::wrkcntinf(job, host, surface),
        "WRKSRVPVD" => service::wrksrvpvd(job, host, surface, r),
        "DSPSRVSTS" => service::dspsrvsts(host, surface, r),
        "DSPPFM" => pfm::dsppfm(host, surface, r),
        "WRKJRN" => jrn::wrkjrn(job, host, surface, r),
        "WRKJRNA" => jrn::wrkjrna(job, host, surface, r),
        "DSPJRN" => jrn::dspjrn(host, surface, r),
        "DSPJRNRCVA" => jrn::dspjrnrcva(host, surface, r),
        _ => return None,
    })
}

/// A parameter's value as typed or defaulted, upper case; `None` when
/// the command has none.
fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::List(items) => items.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        })
        .map(|s| s.trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|s| !s.is_empty())
}

/// A qualified parameter as `LIB/NAME`, the library `lib` when none
/// was given.
fn qual(r: &Resolved, kwd: &str, lib: &str) -> String {
    let up = |v: &Value| v.source().trim().to_ascii_uppercase();
    let (name, l) = match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(up).unwrap_or_default(),
            parts.get(1).map(up).filter(|l| !l.is_empty()),
        ),
        Some(v) => {
            let s = up(v);
            match s.split_once('/') {
                Some((l, n)) => (n.to_string(), Some(l.to_string())),
                None => (s, None),
            }
        }
        None => (String::new(), None),
    };
    format!("{}/{name}", l.unwrap_or_else(|| lib.to_string()))
}

/// A panel group of `QSYS` as a dialog.
fn dialog(host: &Host, group: &str) -> Result<UimDialog, Fail> {
    crate::panels::dialog(host, group)
}

/// Show a panel, a render failure as the command's escape.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| crate::panels::failed(format!("panel {panel}: {e}")))
}

/// The service's refusal as an escape: `CPFnnnn text` keeps its
/// identifier.
fn refused(e: &str) -> Fail {
    let e = e.trim();
    let id = e.split_whitespace().next().unwrap_or("");
    let is_id = id.len() == 7
        && id[..3].bytes().all(|b| b.is_ascii_uppercase())
        && id[3..].bytes().all(|b| b.is_ascii_hexdigit());
    if is_id {
        Fail::new(id, e[7..].trim())
    } else {
        crate::panels::failed(e)
    }
}

/// The entries of a list: `(variable, value)` pairs each.
type Rows = Vec<Vec<(String, String)>>;

/// What a list display's `reload` is.
type Reload<'a> = dyn FnMut(&Host, &mut UimDialog) -> Result<(), Fail> + 'a;

/// What a list display's `keys` is.
type Keys<'a> =
    dyn FnMut(&mut Job, &Host, &mut Surface, &mut UimDialog, Response) -> Result<Step, Fail> + 'a;

/// A display's data through the storage service.
fn fetch(host: &Host, what: &str, args: &[&str]) -> Result<stg::proto::PanelData, Fail> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    host.link
        .borrow_mut()
        .with(|l| l.panel2_data(what.into(), args))
        .map_err(|e| refused(&e))
}

/// Put fetched data in a dialog: the variables, then the entries in
/// `list` (an entry's `*LIST` names another list), every one given,
/// the page kept when it still has entries.
fn load(d: &mut UimDialog, list: &str, data: stg::proto::PanelData) {
    let (vars, rows) = data;
    for (k, v) in &vars {
        d.set(k, v);
    }
    let mut by_list: Vec<(String, Rows)> = vec![(list.to_string(), Vec::new())];
    for mut row in rows {
        let name = match row.iter().position(|(k, _)| k == "*LIST") {
            Some(i) => row.remove(i).1,
            None => list.to_string(),
        };
        match by_list.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => v.push(row),
            None => by_list.push((name, vec![row])),
        }
    }
    for (name, rows) in by_list {
        if !name.is_empty() {
            fill(d, &name, rows);
        }
    }
}

/// Replace a list's entries, the page kept when it still has entries.
fn fill(d: &mut UimDialog, list: &str, rows: Rows) {
    let top = d.list_mut(list).top;
    let n = rows.len();
    let l = d.list_mut(list);
    l.rows = rows;
    l.complete = true;
    l.options.clear();
    l.top = if top < n { top } else { 0 };
}

/// A list entry's value.
fn cell(d: &UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x)
        .unwrap_or_default()
}

/// Run a command for a panel; its first line goes on the message line.
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

/// What a list display does after a key.
enum Step {
    /// Show the panel again.
    Stay,
    /// Fetch the data again, then show it.
    Reload,
    /// Leave the display.
    Leave,
}

/// A list display's loop: `reload` fills the dialog; commands, `CMD`
/// options, F4, F5, F9, F3 and F12 are handled here, the rest by
/// `keys` (an option without a `CMD` action comes as `Options` with
/// that one choice; Enter as `Return(1)`, see [`others`]).
fn work(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    reload: &mut Reload<'_>,
    keys: &mut Keys<'_>,
) -> Result<String, Fail> {
    let mut last = String::new();
    reload(host, d)?;
    loop {
        let resp = show(d, surface, panel)?;
        let step = match resp {
            Response::Exit | Response::Cancel | Response::Gone => Step::Leave,
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                exec(job, host, d, &text);
                Step::Reload
            }
            Response::Retrieve => {
                if last.is_empty() {
                    d.message = Some(("There is no command to retrieve.".into(), true));
                } else {
                    d.set(CMDLINE_VAR, &last);
                }
                Step::Stay
            }
            Response::Return(4) if d.get(CMDLINE_VAR).trim().is_empty() => {
                d.message = Some(("Type a command to prompt on the command line.".into(), true));
                Step::Stay
            }
            Response::Return(5) => Step::Reload,
            Response::Options(choices) => {
                let mut step = Step::Reload;
                for c in &choices {
                    if let Some(Action::Cmd(t)) = &c.action {
                        let text = d.substitute(t, Some((&c.list, c.row)));
                        exec(job, host, d, &text);
                    } else if let Step::Leave =
                        keys(job, host, surface, d, Response::Options(vec![c.clone()]))?
                    {
                        step = Step::Leave;
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
                step
            }
            Response::Menu(m) => {
                exec(job, host, d, &format!("GO {m}"));
                Step::Reload
            }
            other => keys(job, host, surface, d, other)?,
        };
        match step {
            Step::Stay => {}
            Step::Reload => reload(host, d)?,
            Step::Leave => return Ok(String::new()),
        }
    }
}

/// What a list display does with an answer it has nothing more for:
/// Enter shows the data again, the rest is not available.
fn others(d: &mut UimDialog, resp: &Response) -> Step {
    match resp {
        Response::Return(1) => Step::Reload,
        other => no_more(d, other),
    }
}

/// Keys a display has nothing more for.
fn no_more(d: &mut UimDialog, resp: &Response) -> Step {
    match resp {
        Response::Options(c) => {
            let opt = c.first().map_or("", |c| c.option.as_str());
            d.message = Some((
                format!("Option {opt} is not available in this release."),
                true,
            ));
        }
        Response::Return(n) => {
            d.message = Some((format!("F{n} is not available in this release."), true));
        }
        _ => {
            d.message = Some((
                "That function is not available in this release.".into(),
                true,
            ));
        }
    }
    Step::Stay
}

/// Show a display panel until it is left: Enter and F12 go back, F3
/// leaves, F5 runs `reload`.
fn display(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    reload: &mut Reload<'_>,
) -> Result<String, Fail> {
    reload(host, d)?;
    loop {
        match show(d, surface, panel)? {
            Response::Return(5) => reload(host, d)?,
            Response::Exit
            | Response::Cancel
            | Response::Gone
            | Response::Return(_)
            | Response::Command(_) => return Ok(String::new()),
            Response::Options(_)
            | Response::Retrieve
            | Response::Menu(_)
            | Response::Panel(_)
            | Response::Print => {
                d.message = Some((
                    "That function is not available in this release.".into(),
                    true,
                ));
            }
        }
    }
}

/// The choice an `Options` answer carries.
fn choice(resp: &Response) -> Option<&Choice> {
    match resp {
        Response::Options(c) => c.first(),
        _ => None,
    }
}
