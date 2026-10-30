// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP lists: interfaces (`CFGTCP` 1), routes (2), port
//! restrictions (4), the host table (10) and the related tables, each
//! with its option 5 display. A user without `*IOSYSCFG` sees the
//! display-only panels, as the system shows them.

use mi::Job;

use super::{Left, NO_PRINT, cell, dialog, exec, fetch, fill, may_change, option_command, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

/// What an option without a `CMD` action does on a list.
type Special<'a> = &'a mut dyn FnMut(&mut Job, &Host, &mut Surface, &mut UimDialog, &Choice) -> Result<Option<Left>, Fail>;

/// One list: its group, panels, data and list names, and what its
/// `RETURN` options do.
struct Spec<'a> {
    group: &'static str,
    full: &'static str,
    only: &'static str,
    what: &'static str,
    list: &'static str,
    views: usize,
    special: Special<'a>,
}

/// Show a list until it is left.
#[allow(clippy::needless_pass_by_value)] // the spec is the call's own
fn work(job: &mut Job, host: &Host, surface: &mut Surface, spec: Spec<'_>) -> Result<Left, Fail> {
    let mut d = dialog(host, spec.group, false)?;
    let panel = if may_change(host) { spec.full } else { spec.only };
    let mut args: Vec<String> = Vec::new();
    let mut last = String::new();
    loop {
        match fetch(host, spec.what, &args) {
            Ok(data) => fill(&mut d, &data),
            Err(m) => d.message = Some(m),
        }
        let resp = show(&mut d, surface, panel)?;
        match resp {
            Response::Exit => return Ok(Left::Exit),
            Response::Cancel => return Ok(Left::Cancel),
            Response::Gone => return Ok(Left::Gone),
            Response::Options(choices) => {
                for c in &choices {
                    if let Some(text) = option_command(&d, c) {
                        if let Some(m) = exec(job, host, &text) {
                            d.message = Some(m);
                        }
                    } else {
                        match (spec.special)(job, host, surface, &mut d, c)? {
                            Some(Left::Exit) => return Ok(Left::Exit),
                            Some(Left::Gone) => return Ok(Left::Gone),
                            Some(Left::Cancel) | None => {}
                        }
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            Response::Return(11) if spec.views > 1 => {
                d.view = (d.view + 1) % spec.views;
                d.set("VIEW", &(d.view + 1).to_string());
            }
            Response::Return(13) => {
                // The host table in the order entries were added, and back.
                args = if args.is_empty() { vec!["ADD".into()] } else { Vec::new() };
            }
            Response::Return(17) => d.list_mut(spec.list).top = 0,
            Response::Return(18) => {
                let n = d.list_ref(spec.list).map_or(0, |l| l.rows.len());
                d.list_mut(spec.list).top = n.saturating_sub(1);
            }
            Response::Return(6) | Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
            }
            Response::Retrieve => d.set(crate::display::uim::CMDLINE_VAR, &last),
            Response::Return(_) | Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// An option 5 display: its data shown until it is left.
fn display(
    host: &Host,
    surface: &mut Surface,
    group: &'static str,
    panel: &str,
    what: &str,
    args: &[String],
) -> Result<Option<Left>, Fail> {
    let mut d = dialog(host, group, false)?;
    match fetch(host, what, args) {
        Ok(data) => fill(&mut d, &data),
        Err(m) => {
            d.message = Some(m);
        }
    }
    loop {
        match show(&mut d, surface, panel)? {
            Response::Exit => return Ok(Some(Left::Exit)),
            Response::Gone => return Ok(Some(Left::Gone)),
            Response::Cancel | Response::Return(0) => return Ok(None),
            Response::Return(22) => {
                d.message = Some(("Every field is shown whole on this display.".into(), false));
            }
            _ => {}
        }
    }
}

fn not_valid(d: &mut UimDialog, c: &Choice) -> Option<Left> {
    d.message = Some((format!("Option {} is not valid here.", c.option), true));
    None
}

/// `CFGTCP` option 1.
pub fn ifcs(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    work(
        job,
        host,
        surface,
        Spec {
            group: "QTOCIFC",
            full: "WRKIFC",
            only: "WRKIFCD",
            what: "IFC",
            list: "IFCS",
            views: 3,
            special: &mut |_, host, surface, d, c| match c.option.as_str() {
                "5" => {
                    let key = cell(d, &c.list, c.row, "IFCADR");
                    display(host, surface, "QTOCIFC", "DSPIFC", "IFC5", &[key])
                }
                _ => Ok(not_valid(d, c)),
            },
        },
    )
}

/// A route's key from its list entry, as the commands take it.
fn route_parms(d: &UimDialog, c: &Choice) -> (String, Vec<String>) {
    let dest = cell(d, &c.list, c.row, "RTEDEST");
    let mask = cell(d, &c.list, c.row, "RTEMASK");
    let tos = cell(d, &c.list, c.row, "RTETOS");
    let hop = cell(d, &c.list, c.row, "RTEHOP");
    let parm = format!("RTEDEST('{dest}') SUBNETMASK('{mask}') TOS({tos}) NEXTHOP('{hop}')")
        .replace("('*", "(*")
        .replace("*NONE')", "*NONE)")
        .replace("ROUTE')", "ROUTE)")
        .replace("MCAST')", "MCAST)");
    (parm, vec![dest, mask, tos, hop])
}

/// `CFGTCP` option 2.
pub fn rtes(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    work(
        job,
        host,
        surface,
        Spec {
            group: "QTOCRTE",
            full: "WRKRTE",
            only: "WRKRTED",
            what: "RTE",
            list: "RTES",
            views: 3,
            special: &mut |job, host, surface, d, c| {
                let (parm, args) = route_parms(d, c);
                match c.option.as_str() {
                    "2" => {
                        d.message = exec(job, host, &format!("?CHGTCPRTE {parm}"));
                        Ok(None)
                    }
                    "4" => {
                        d.message = exec(job, host, &format!("RMVTCPRTE {parm}"));
                        Ok(None)
                    }
                    "5" => display(host, surface, "QTOCRTE", "DSPRTE", "RTE5", &args),
                    _ => Ok(not_valid(d, c)),
                }
            },
        },
    )
}

/// `CFGTCP` option 4.
pub fn ports(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    work(
        job,
        host,
        surface,
        Spec {
            group: "QTOCPORT",
            full: "WRKPORT",
            only: "WRKPORTD",
            what: "PORT",
            list: "PORTS",
            views: 1,
            special: &mut |job, host, _, d, c| match c.option.as_str() {
                "4" => {
                    let lower = cell(d, &c.list, c.row, "PLOWER");
                    let upper = cell(d, &c.list, c.row, "PUPPER");
                    let proto = cell(d, &c.list, c.row, "PPROTO");
                    let user = cell(d, &c.list, c.row, "PUSER");
                    d.message = exec(
                        job,
                        host,
                        &format!("RMVTCPPORT PORT({lower} {upper}) PROTOCOL({proto}) USRPRF({user})"),
                    );
                    Ok(None)
                }
                _ => Ok(not_valid(d, c)),
            },
        },
    )
}

/// `CFGTCP` option 10.
pub fn hosts(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    work(
        job,
        host,
        surface,
        Spec {
            group: "QTOCHTE",
            full: "WRKHTE",
            only: "WRKHTED",
            what: "HTE",
            list: "HOSTS",
            views: 1,
            special: &mut |_, host, surface, d, c| match c.option.as_str() {
                "5" => {
                    let key = cell(d, &c.list, c.row, "HTEKEY");
                    display(host, surface, "QTOCHTE", "DSPHTE", "HTE5", &[key])
                }
                _ => Ok(not_valid(d, c)),
            },
        },
    )
}

/// The related tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tbl {
    /// `WRKSRVTBLE`.
    Srv,
    /// `WRKPCLTBLE`.
    Pcl,
    /// `WRKNETTBLE`.
    Net,
}

/// `WRKSRVTBLE`, `WRKPCLTBLE`, `WRKNETTBLE`.
pub fn table(job: &mut Job, host: &Host, surface: &mut Surface, t: Tbl) -> Result<Left, Fail> {
    let (full, only, what, list) = match t {
        Tbl::Srv => ("WRKSRV", "WRKSRVD", "SRV", "SVCS"),
        Tbl::Pcl => ("WRKPCL", "WRKPCLD", "PCL", "PCLS"),
        Tbl::Net => ("WRKNET", "WRKNETD", "NET", "NETS"),
    };
    work(
        job,
        host,
        surface,
        Spec {
            group: "QTOCTBL",
            full,
            only,
            what,
            list,
            views: 1,
            special: &mut |_, _, _, d, c| match c.option.as_str() {
                "5" => {
                    let vals: Vec<String> = d
                        .row(&c.list, c.row)
                        .into_iter()
                        .filter(|(k, _)| k != "OPT" && k != "SP")
                        .map(|(_, v)| v.trim().to_string())
                        .collect();
                    d.message = Some((vals.join("  "), false));
                    Ok(None)
                }
                _ => Ok(not_valid(d, c)),
            },
        },
    )
}
