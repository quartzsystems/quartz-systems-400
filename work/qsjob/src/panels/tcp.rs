// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP configuration displays: the `CFGTCP` menu and its lists
//! (interfaces, routes, port restrictions, the host table), the
//! related tables (`WRKSRVTBLE`, `WRKPCLTBLE`, `WRKNETTBLE`), the
//! `CFGTCPAPP` menu with the applications' menus, and `CFGTCPPTP`.
//! The data comes from the storage service (`stg::tcp`); the panel
//! groups are the shipped `QTOC*`.

mod lists;
mod menus;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::tcp::{PanelData, TcpOp, TcpReply};

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

/// How a display was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// F12: back to the display before.
    Cancel,
    /// F3: out of the function.
    Exit,
    /// The display went away.
    Gone,
}

/// The driver of a command, if it is one of these.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    let left = match r.name.as_str() {
        "CFGTCP" => menus::cfgtcp(job, host, surface),
        "CFGTCPAPP" => menus::app(job, host, surface, "CFGTCPAPP"),
        "CFGTCPSNMP" => menus::app(job, host, surface, "CFGSNMP"),
        "CFGTCPRTD" => menus::app(job, host, surface, "CFGRTD"),
        "CFGTCPBP" => menus::app(job, host, surface, "CFGBP"),
        "CFGTCPTELN" => menus::app(job, host, surface, "CFGTELN"),
        "CFGTCPSMTP" => menus::app(job, host, surface, "CFGSMTP"),
        "CFGTCPPTP" => menus::ptp(job, host, surface),
        "WRKSRVTBLE" => lists::table(job, host, surface, lists::Tbl::Srv),
        "WRKPCLTBLE" => lists::table(job, host, surface, lists::Tbl::Pcl),
        "WRKNETTBLE" => lists::table(job, host, surface, lists::Tbl::Net),
        _ => return None,
    };
    Some(left.map(|_| String::new()))
}

/// A dialog over a `QTOC*` group.
fn dialog(host: &Host, group: &str, copyright: bool) -> Result<UimDialog, Fail> {
    let mut d = crate::panels::dialog(host, group)?;
    d.copyright = copyright;
    Ok(d)
}

/// Show a panel, a render failure as the command's escape.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| Fail::new("CPF6A12", format!("panel {panel}: {e}")))
}

/// A panel's data from the service.
fn fetch(host: &Host, what: &str, args: &[String]) -> Result<PanelData, (String, bool)> {
    match host
        .link
        .borrow_mut()
        .with(|l| l.tcp(TcpOp::Panel { what: what.into(), args: args.to_vec() }))
    {
        TcpReply::Panel { data } => Ok(data),
        TcpReply::Failed { note } => Err((note.text, true)),
        _ => Err(("The storage service gave no panel data.".into(), true)),
    }
}

/// The data's variables and lists into the dialog, a list's page kept
/// when it still has entries.
fn fill(d: &mut UimDialog, data: &PanelData) {
    for (k, v) in &data.0 {
        d.set(k, v);
    }
    for (name, rows) in &data.1 {
        let top = d.list_mut(name).top;
        d.clear_list(name);
        let n = rows.len();
        for r in rows {
            d.add_row(name, r.clone());
        }
        let l = d.list_mut(name);
        l.complete = true;
        l.top = if top < n { top } else { 0 };
    }
}

/// A list entry's value.
fn cell(d: &UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x.trim().to_string())
        .unwrap_or_default()
}

/// Whether a command exists, so an option of an application this
/// release does not have says so.
fn exists(host: &Host, text: &str) -> bool {
    let name = text
        .trim()
        .trim_start_matches('?')
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    host.link
        .borrow_mut()
        .with(|l| l.cmd_lookup(name, None))
        .is_ok_and(|f| f.is_some())
}

/// Run a command from a panel: the line to show after it, an error
/// when it failed or does not exist here. A leading `?` prompts.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    if !exists(host, text) {
        let name = text.trim().trim_start_matches('?').split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        return Some((format!("{name} is not available in this release."), true));
    }
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        if f.msgid == "CPF6801" {
            return None;
        }
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        let line = if line.is_empty() { format!("{} ended with {}.", out.name, f.msgid) } else { line };
        return Some((line, true));
    }
    out.text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| (l.to_string(), false))
}

/// A `CMD` option's command with the entry's values.
fn option_command(d: &UimDialog, c: &Choice) -> Option<String> {
    match &c.action {
        Some(uim::model::Action::Cmd(t)) => Some(d.substitute(t, Some((&c.list, c.row)))),
        _ => None,
    }
}

/// Whether the user may change the configuration (`*IOSYSCFG`): the
/// panels with every option, or the display-only ones.
fn may_change(host: &Host) -> bool {
    let p = host.link.borrow_mut().with(Client::whoami);
    p.has(sec::profile::SpecialAuth::IOSYSCFG)
}

/// Print is not there yet.
const NO_PRINT: &str = "Print is not available in this release.";
