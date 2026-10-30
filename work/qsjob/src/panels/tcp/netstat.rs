// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NETSTAT` (and `WRKTCPSTS`) at a display: the menu, the interface,
//! route and connection lists for IPv4 and IPv6 with their option 5
//! displays, option 8 (an interface's routes, a connection's jobs),
//! option 4 on a connection and 9/10 on an interface for a user with
//! `*IOSYSCFG`, and the stack status. The data comes from the storage
//! service (`TcpOp::Netstat`); the panel group is `QTOCNETS`.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::tcp::{PanelData, TcpOp, TcpReply};

use super::{Left, NO_PRINT, cell, dialog, exec, fill, may_change, option_command, show};
use crate::command::{Fail, parms_of};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

const GROUP: &str = "QTOCNETS";

/// `OPTION` as its number: `*SELECT` 0, `*IFC` 1 ... `*STATUS` 10.
pub fn option(r: &Resolved) -> u32 {
    let t = parms_of(r).name("OPTION");
    match t.as_str() {
        "*IFC" => 1,
        "*RTE" => 2,
        "*CNN" => 3,
        "*IFC6" => 4,
        "*RTE6" => 5,
        "*CNN6" => 6,
        "*STATUS" => 10,
        n => n.parse().unwrap_or(0),
    }
}

/// The driver: `None` for another command.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<Left, Fail>> {
    if r.name != "NETSTAT" && r.name != "WRKTCPSTS" {
        return None;
    }
    Some(match option(r) {
        0 => menu(job, host, surface),
        n => view(job, host, surface, n),
    })
}

/// The data of a view.
fn fetch(host: &Host, what: &str, args: &[String]) -> Result<PanelData, (String, bool)> {
    match host
        .link
        .borrow_mut()
        .with(|l| l.tcp(TcpOp::Netstat { what: what.into(), args: args.to_vec() }))
    {
        TcpReply::Panel { data } => Ok(data),
        TcpReply::Failed { note } => Err((note.text, true)),
        _ => Err(("The storage service gave no network status.".into(), true)),
    }
}

/// Option `n` of the menu.
fn view(job: &mut Job, host: &Host, surface: &mut Surface, n: u32) -> Result<Left, Fail> {
    match n {
        1 => list(job, host, surface, Kind::Ifc(false), None),
        2 => list(job, host, surface, Kind::Rte(false), None),
        3 => list(job, host, surface, Kind::Cnn(false), None),
        4 => list(job, host, surface, Kind::Ifc(true), None),
        5 => list(job, host, surface, Kind::Rte(true), None),
        6 => list(job, host, surface, Kind::Cnn(true), None),
        _ => status(job, host, surface),
    }
}

fn menu(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    let mut d = dialog(host, GROUP, false)?;
    let mut last = String::new();
    loop {
        match show(&mut d, surface, "NETSTAT")? {
            Response::Exit => return Ok(Left::Exit),
            Response::Cancel => return Ok(Left::Cancel),
            Response::Gone => return Ok(Left::Gone),
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(4) => {
                let typed = d.get(CMDLINE_VAR).trim().to_string();
                if typed.is_empty() {
                    d.message = Some(("Type a command to prompt on the command line.".into(), true));
                } else {
                    last.clone_from(&typed);
                    d.message = exec(job, host, &format!("?{}", typed.trim_start_matches('?')));
                    d.set(CMDLINE_VAR, "");
                }
            }
            Response::Return(n @ 101..=110) => match view(job, host, surface, n - 100)? {
                Left::Exit => return Ok(Left::Exit),
                Left::Gone => return Ok(Left::Gone),
                Left::Cancel => {}
            },
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// A list of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Interfaces, IPv6 when true.
    Ifc(bool),
    /// Routes.
    Rte(bool),
    /// Connections.
    Cnn(bool),
}

impl Kind {
    fn v6(self) -> bool {
        matches!(self, Kind::Ifc(true) | Kind::Rte(true) | Kind::Cnn(true))
    }

    /// The other address family's list (F20).
    fn other(self) -> Self {
        match self {
            Kind::Ifc(v) => Kind::Ifc(!v),
            Kind::Rte(v) => Kind::Rte(!v),
            Kind::Cnn(v) => Kind::Cnn(!v),
        }
    }

    fn what(self) -> &'static str {
        match self {
            Kind::Ifc(false) => "IFC",
            Kind::Ifc(true) => "IFC6",
            Kind::Rte(false) => "RTE",
            Kind::Rte(true) => "RTE6",
            Kind::Cnn(false) => "CNN",
            Kind::Cnn(true) => "CNN6",
        }
    }

    fn list(self) -> &'static str {
        match self {
            Kind::Ifc(false) => "IFCS",
            Kind::Ifc(true) => "IFC6S",
            Kind::Rte(false) => "RTES",
            Kind::Rte(true) => "RTE6S",
            Kind::Cnn(false) => "CNNS",
            Kind::Cnn(true) => "CNN6S",
        }
    }

    /// The panel: with the changing options for `*IOSYSCFG`.
    fn panel(self, full: bool) -> &'static str {
        match (self, full) {
            (Kind::Ifc(false), true) => "IFCSTS",
            (Kind::Ifc(false), false) => "IFCSTSD",
            (Kind::Ifc(true), true) => "IFC6STS",
            (Kind::Ifc(true), false) => "IFC6STSD",
            (Kind::Rte(false), _) => "RTEINF",
            (Kind::Rte(true), _) => "RTE6INF",
            (Kind::Cnn(false), true) => "CNNSTS",
            (Kind::Cnn(false), false) => "CNNSTSD",
            (Kind::Cnn(true), true) => "CNN6STS",
            (Kind::Cnn(true), false) => "CNN6STSD",
        }
    }

    fn views(self) -> usize {
        match self {
            Kind::Cnn(_) => 3,
            _ => 2,
        }
    }

    /// The variable holding an entry's key.
    fn key(self) -> &'static str {
        match self {
            Kind::Ifc(false) => "IADR",
            Kind::Ifc(true) => "XADR",
            Kind::Rte(false) => "RKEY",
            Kind::Rte(true) => "YKEY",
            Kind::Cnn(false) => "CKEY",
            Kind::Cnn(true) => "ZKEY",
        }
    }
}

/// Show a list until it is left; `of` narrows routes to an interface.
fn list(job: &mut Job, host: &Host, surface: &mut Surface, start: Kind, of: Option<&str>) -> Result<Left, Fail> {
    let mut kind = start;
    let full = may_change(host);
    let mut d = dialog(host, GROUP, false)?;
    let mut last = String::new();
    loop {
        let (what, args) = match of {
            Some(a) => ("RTEOF", vec![a.to_string()]),
            None => (kind.what(), Vec::new()),
        };
        match fetch(host, what, &args) {
            Ok(data) => fill(&mut d, &data),
            Err(m) => d.message = Some(m),
        }
        match show(&mut d, surface, kind.panel(full))? {
            Response::Exit => return Ok(Left::Exit),
            Response::Cancel => return Ok(Left::Cancel),
            Response::Gone => return Ok(Left::Gone),
            Response::Options(choices) => {
                for c in &choices {
                    if let Some(text) = option_command(&d, c) {
                        d.message = exec(job, host, &text);
                        continue;
                    }
                    let key = cell(&d, &c.list, c.row, kind.key());
                    let left = match (kind, c.option.as_str()) {
                        (_, "5") => detail(job, host, surface, kind, &key)?,
                        (Kind::Ifc(_), "8") => Some(list(job, host, surface, Kind::Rte(kind.v6()), Some(&key))?),
                        (Kind::Ifc(_), "12") => {
                            let line = cell(&d, &c.list, c.row, if kind.v6() { "XLIND" } else { "ILIND" });
                            d.message = if line.starts_with('*') {
                                Some((format!("{line} has no configuration status."), true))
                            } else {
                                exec(job, host, &format!("WRKCFGSTS CFGTYPE(*LIN) CFGD({line})"))
                            };
                            None
                        }
                        (Kind::Ifc(_), "14") => {
                            d.message = Some(("Multicast groups are not shown in this release.".into(), true));
                            None
                        }
                        (Kind::Cnn(_), "8") => jobs(job, host, surface, &key)?,
                        (Kind::Cnn(_), "4") => {
                            d.message = Some(end_cnn(host, &key));
                            None
                        }
                        _ => {
                            d.message = Some((format!("Option {} is not valid here.", c.option), true));
                            None
                        }
                    };
                    match left {
                        Some(Left::Exit) => return Ok(Left::Exit),
                        Some(Left::Gone) => return Ok(Left::Gone),
                        Some(Left::Cancel) | None => {}
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            Response::Return(11) => {
                d.view = (d.view + 1) % kind.views();
                d.set("VIEW", &(d.view + 1).to_string());
            }
            Response::Return(14) if matches!(kind, Kind::Cnn(_)) => {
                // Port numbers: the view that shows them.
                d.view = 2;
                d.set("VIEW", "3");
            }
            Response::Return(20) if of.is_none() => {
                kind = kind.other();
                d.view = 0;
                d.set("VIEW", "1");
                d.list_mut(kind.list()).top = 0;
            }
            Response::Return(17) => d.list_mut(kind.list()).top = 0,
            Response::Return(18) => {
                let n = d.list_ref(kind.list()).map_or(0, |l| l.rows.len());
                d.list_mut(kind.list()).top = n.saturating_sub(1);
            }
            Response::Return(9) => d.message = command_line(job, host),
            Response::Return(13 | 15 | 22) => {
                d.message = Some(("That function is not available in this release.".into(), true));
            }
            Response::Return(6) | Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            _ => {}
        }
    }
}

/// F9: Command Entry, until it is left.
fn command_line(job: &mut Job, host: &Host) -> Option<(String, bool)> {
    exec(job, host, "CALL PGM(QCMD)")
}

/// `NETSTAT` option 4 on a connection.
fn end_cnn(host: &Host, key: &str) -> (String, bool) {
    match host.link.borrow_mut().with(|l| l.tcp(TcpOp::EndCnn { key: key.into() })) {
        TcpReply::Done { notes, .. } => notes.last().map_or_else(|| (String::new(), false), |n| (n.text.clone(), false)),
        TcpReply::Failed { note } => (note.text, true),
        _ => ("The storage service gave no answer.".into(), true),
    }
}

/// What a display does with a `RETURN n` key of its own.
type OnKey<'a> = &'a mut dyn FnMut(&mut Job, &Host, &mut Surface, &mut UimDialog, u32) -> Result<Option<Left>, Fail>;

/// A display panel until it is left; `on` handles its other keys.
fn display(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    panel: &str,
    what: &str,
    key: &str,
    on: OnKey<'_>,
) -> Result<Option<Left>, Fail> {
    let mut d = dialog(host, GROUP, false)?;
    let args = vec![key.to_string()];
    loop {
        match fetch(host, what, &args) {
            Ok(data) => fill(&mut d, &data),
            Err(m) => d.message = Some(m),
        }
        match show(&mut d, surface, panel)? {
            Response::Exit => return Ok(Some(Left::Exit)),
            Response::Gone => return Ok(Some(Left::Gone)),
            Response::Cancel | Response::Return(0) => return Ok(None),
            Response::Return(5) => {}
            Response::Return(9) => d.message = command_line(job, host),
            Response::Return(6) | Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Return(22) => d.message = Some(("Every field is shown whole on this display.".into(), false)),
            Response::Return(n) => {
                if let Some(l) = on(job, host, surface, &mut d, n)? {
                    return Ok(Some(l));
                }
            }
            _ => {}
        }
    }
}

/// Option 5 on an entry.
fn detail(job: &mut Job, host: &Host, surface: &mut Surface, kind: Kind, key: &str) -> Result<Option<Left>, Fail> {
    match kind {
        Kind::Ifc(_) => display(job, host, surface, "DSPIFC", "IFC5", key, &mut |_, _, _, _, _| Ok(None)),
        Kind::Rte(_) => {
            let panel = if key.starts_with("*DFTROUTE") { "DSPDFT" } else { "DSPRTE" };
            display(job, host, surface, panel, "RTE5", key, &mut |_, _, _, _, _| Ok(None))
        }
        Kind::Cnn(_) => {
            let k = key.to_string();
            display(job, host, surface, "DSPCNN", "CNN5", key, &mut |job, host, surface, d, n| {
                match n {
                    8 => return jobs(job, host, surface, &k),
                    14 => d.message = Some(("The ports are shown by name and number in the list (F11).".into(), false)),
                    _ => {}
                }
                Ok(None)
            })
        }
    }
}

/// Option 8 on a connection: the jobs holding it.
fn jobs(job: &mut Job, host: &Host, surface: &mut Surface, key: &str) -> Result<Option<Left>, Fail> {
    let mut d = dialog(host, GROUP, false)?;
    let args = vec![key.to_string()];
    loop {
        match fetch(host, "CNN8", &args) {
            Ok(data) => fill(&mut d, &data),
            Err(m) => d.message = Some(m),
        }
        match show(&mut d, surface, "DSPJOBS")? {
            Response::Exit => return Ok(Some(Left::Exit)),
            Response::Gone => return Ok(Some(Left::Gone)),
            Response::Cancel => return Ok(None),
            Response::Options(choices) => {
                for c in &choices {
                    if let Some(text) = option_command(&d, c) {
                        d.message = exec(job, host, &text);
                    }
                }
                d.list_mut("JOBS").options.clear();
            }
            Response::Return(9) => d.message = command_line(job, host),
            Response::Return(6) | Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// Option 10.
fn status(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    Ok(display(job, host, surface, "DSPSTS", "STS", "", &mut |_, _, _, _, _| Ok(None))?.unwrap_or(Left::Cancel))
}
