// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program displays: `DSPPGM`, `DSPSRVPGM` and `DSPMOD` one display
//! per detail (Enter goes on, F12 back), `DSPBNDDIR`, `DSPCMD` and
//! `DSPPGMREF` (the report's lines).

use cmd::Resolved;
use mi::Job;

use super::{
    NO_PRINT, act, dashed, dialog, fetch, fill, list_of, parm, parm_list, qual, service_text,
    set_vars, show, var_of,
};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// One display of a program: its detail and how to show it.
struct Detail {
    /// The `DETAIL` value.
    name: &'static str,
    /// The panel (without its `PGM`/`SRV` prefix).
    panel: &'static str,
    /// For an export-style list: its heading, the data's list, the
    /// list view and the text when it is empty.
    list: Option<(&'static str, &'static str, usize, &'static str)>,
}

const fn plain(name: &'static str, panel: &'static str) -> Detail {
    Detail {
        name,
        panel,
        list: None,
    }
}

const fn listed(
    name: &'static str,
    head: &'static str,
    data: &'static str,
    view: usize,
    empty: &'static str,
) -> Detail {
    Detail {
        name,
        panel: "EXP",
        list: Some((head, data, view, empty)),
    }
}

/// A program's displays in order.
const PGM_DETAILS: &[Detail] = &[
    plain("*BASIC", "BASIC"),
    plain("*SIZE", "SIZE"),
    plain("*MODULE", "MOD"),
    plain("*SRVPGM", "SRV"),
    listed(
        "*ACTGRPEXP",
        "Activation group exports:",
        "",
        1,
        "(No activation group exports)",
    ),
    listed(
        "*ACTGRPIMP",
        "Activation group imports:",
        "",
        1,
        "(No activation group imports)",
    ),
    listed("*COPYRIGHT", "Copyrights:", "", 0, "(No copyrights)"),
];

/// A service program's displays in order.
const SRV_DETAILS: &[Detail] = &[
    plain("*BASIC", "BASIC"),
    plain("*SIZE", "SIZE"),
    plain("*MODULE", "MOD"),
    plain("*SRVPGM", "SRV"),
    listed(
        "*PROCEXP",
        "Procedure Exports:",
        "PROCEXP",
        0,
        "(No procedure exports)",
    ),
    listed("*DTAEXP", "Data Exports:", "DATAEXP", 1, "(No data exports)"),
    listed(
        "*ACTGRPEXP",
        "Activation group exports:",
        "",
        1,
        "(No activation group exports)",
    ),
    listed(
        "*ACTGRPIMP",
        "Activation group imports:",
        "",
        1,
        "(No activation group imports)",
    ),
    listed("*SIGNATURE", "Signatures:", "SIGS", 0, "(No signatures)"),
    listed("*COPYRIGHT", "Copyrights:", "", 0, "(No copyrights)"),
];

/// A heading centered over the display.
fn centered(text: &str) -> String {
    format!("{}{text}", " ".repeat(80usize.saturating_sub(text.len()) / 2 - 1))
}

/// `Display n of m`, right-aligned in 15.
fn position(i: usize, n: usize) -> String {
    format!("{:>15}", format!("Display {i} of {n}"))
}

/// The hexadecimal signature as characters (CP037), unprintable bytes
/// as periods.
fn sig_chars(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect();
    bytes
        .iter()
        .map(|&b| {
            let c = ccsid::char_of(b);
            if c.is_ascii_graphic() || c == ' ' { c } else { '.' }
        })
        .collect()
}

/// `DSPPGM` and `DSPSRVPGM`: each detail asked for, one display at a
/// time.
pub fn dsppgm(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    cmd: &Resolved,
    srvpgm: bool,
) -> Option<Result<String, Fail>> {
    let (name, lib) = qual(cmd, if srvpgm { "SRVPGM" } else { "PGM" });
    let data = read!(&cmd.name, fetch(host, if srvpgm { "SRVPGM" } else { "PGM" }, &[&lib, &name]));
    let all = if srvpgm { SRV_DETAILS } else { PGM_DETAILS };
    let asked = parm_list(cmd, "DETAIL");
    let details: Vec<&Detail> = if asked.is_empty() || asked.iter().any(|a| a == "*ALL") {
        all.iter().collect()
    } else {
        all.iter().filter(|d| asked.iter().any(|a| a == d.name)).collect()
    };
    if details.is_empty() {
        return Some(Err(Fail::later(&cmd.name)));
    }
    let mut d = match dialog(host, "QUIDSPPGM") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    set_vars(&mut d, &data);
    fill(&mut d, "MODS", list_of(&data, "MODS"));
    fill(&mut d, "SRVS", list_of(&data, "SRVS"));
    let hex = list_of(&data, "SRVS");
    let mut chars = false;
    let prefix = if srvpgm { "SRV" } else { "PGM" };
    let count = details.len();
    let mut at = 0;
    while at < count {
        let det = details[at];
        d.set("DSPPOS", &position(at + 1, count));
        d.set("DETAIL", det.name);
        d.copyright = at == 0;
        d.view = 0;
        d.page = 0;
        if let Some((head, list, view, empty)) = det.list {
            d.set("EXPHEAD", &centered(head));
            d.set("EXPEMPTY", empty);
            let rows = if list.is_empty() {
                Vec::new()
            } else {
                list_of(&data, list)
            };
            fill(&mut d, "EXPS", rows);
            d.view = view;
        }
        let panel = format!("{prefix}{}", det.panel);
        let resp = match show(&mut d, surface, &panel) {
            Ok(resp) => resp,
            Err(e) => return Some(Err(e)),
        };
        match resp {
            Response::Exit | Response::Gone => return Some(Ok(String::new())),
            Response::Cancel => {
                if at == 0 {
                    return Some(Ok(String::new()));
                }
                at -= 1;
            }
            Response::Options(c) => act(job, host, &mut d, &c, &|d, c| {
                let get = |v: &str| super::cell(d, &c.list, c.row, v);
                match (c.list.as_str(), c.option.as_str()) {
                    ("MODS", "5") => Some(format!(
                        "DSPMOD MODULE({}/{})",
                        get("MODLIB"),
                        get("MODNAME")
                    )),
                    ("SRVS", "5") => Some(format!(
                        "DSPSRVPGM SRVPGM({}/{})",
                        get("SRVLIB"),
                        get("SRVNAME")
                    )),
                    _ => None,
                }
            }),
            Response::Return(11) => {
                // The signatures as characters or hexadecimal.
                chars = !chars;
                let rows = hex
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|(k, v)| {
                                let shown = if k == "SRVSIG" && chars {
                                    sig_chars(v)
                                } else {
                                    v.clone()
                                };
                                (k.clone(), shown)
                            })
                            .collect()
                    })
                    .collect();
                fill(&mut d, "SRVS", rows);
                let sig = var_of(&data, "CURSIG");
                d.set("CURSIG", &if chars { sig_chars(&sig) } else { sig });
            }
            Response::Return(4) => {
                d.message = Some(("Type an option or press Enter.".into(), true));
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => at += 1,
        }
    }
    Some(Ok(String::new()))
}

/// `DSPMOD`: the module's attributes, then its procedures.
pub fn dspmod(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let (name, lib) = qual(r, "MODULE");
    let data = read!(&r.name, fetch(host, "MODULE", &[&lib, &name]));
    let detail = parm(r, "DETAIL");
    let panels: Vec<(&str, &str)> = match detail.as_str() {
        "*BASIC" => vec![("MODBASIC", "*BASIC")],
        "*PROCLIST" | "*EXPORT" | "*IMPORT" => vec![("MODPROC", "*PROCLIST")],
        _ => vec![("MODBASIC", "*BASIC"), ("MODPROC", "*PROCLIST")],
    };
    let mut d = match dialog(host, "QUIDSPPGM") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    set_vars(&mut d, &data);
    fill(&mut d, "PROCS", list_of(&data, "PROCS"));
    Some(walk(surface, &mut d, &panels))
}

/// Panels one after another: Enter goes on, F12 back, F3 out.
fn walk(surface: &mut Surface, d: &mut UimDialog, panels: &[(&str, &str)]) -> Result<String, Fail> {
    let n = panels.len();
    let mut i = 0;
    while i < n {
        let (panel, detail) = panels[i];
        d.set("DSPPOS", &position(i + 1, n));
        d.set("DETAIL", detail);
        d.copyright = i == 0;
        d.page = 0;
        match show(d, surface, panel)? {
            Response::Exit | Response::Gone => return Ok(String::new()),
            Response::Cancel => {
                if i == 0 {
                    return Ok(String::new());
                }
                i -= 1;
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            Response::Return(0) => i += 1,
            _ => {}
        }
    }
    Ok(String::new())
}

/// `DSPBNDDIR`: the entries in binding order.
pub fn dspbnddir(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let (name, lib) = qual(r, "BNDDIR");
    let data = read!(&r.name, fetch(host, "BNDDIR", &[&lib, &name]));
    let mut d = match dialog(host, "QUIDSPBNDD") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    set_vars(&mut d, &data);
    d.set("CRTHEAD", &dashed("Creation", 23));
    fill(&mut d, "ENTS", list_of(&data, "ENTS"));
    Some(until_left(surface, &mut d, "DSPBNDDIR"))
}

/// Show a display panel until the operator leaves it.
fn until_left(surface: &mut Surface, d: &mut UimDialog, panel: &str) -> Result<String, Fail> {
    loop {
        match show(d, surface, panel)? {
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(0) => {
                return Ok(String::new());
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// `DSPCMD`: the command definition's attributes.
pub fn dspcmd(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let (name, lib) = qual(r, "CMD");
    let data = read!(&r.name, fetch(host, "CMD", &[&lib, &name]));
    let mut d = match dialog(host, "QUIDSPCMD") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    set_vars(&mut d, &data);
    Some(until_left(surface, &mut d, "DSPCMD"))
}

/// The lines of a report in the viewer, F19 and F20 shifting them by
/// 20 columns.
pub(super) fn view_report(
    host: &Host,
    surface: &mut Surface,
    title: &str,
    file: &str,
    text: &str,
) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIRPTVW")?;
    d.copyright = false;
    d.set("RPTTITLE", title);
    d.set("RPTFILE", file);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut shift = 0usize;
    loop {
        d.set("RPTCOLS", &format!("{} - {}", shift + 1, shift + 78));
        let rows = lines
            .iter()
            .map(|l| {
                let s: String = l.chars().skip(shift).collect();
                vec![("LINE".to_string(), s)]
            })
            .collect();
        fill(&mut d, "LINES", rows);
        match show(&mut d, surface, "RPTVIEW")? {
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(0) => {
                return Ok(String::new());
            }
            Response::Return(19) => shift = shift.saturating_sub(20),
            Response::Return(20) => {
                let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                if shift + 78 < widest {
                    shift += 20;
                }
            }
            _ => {}
        }
    }
}

/// `DSPPGMREF`: what the program refers to, as its report shows it.
pub fn dsppgmref(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let text = service_text(host, &r.text())?;
    view_report(
        host,
        surface,
        "Display Program References",
        "QPDSPPGM",
        &text,
    )
}
