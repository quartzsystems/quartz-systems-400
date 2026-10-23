// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object displays: `WRKLIB` (`QUIWRKLIB`), `WRKOBJLCK` (`QGWCOLCK`),
//! `WRKOBJPGP` (`QUIWRKOPGP`), `DSPOVR` (`QUIDSPOVR`), and the panels of
//! functions to come: `DSPSAVF` (`QUIDSPSAVF`), `WRKPRB` (`QGSXPNLS`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{
    Step, cell, choice, dialog, display, exec, fetch, fill, load, others, parm, qual, show, work,
};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

/// F17 and F16 on a list sorted by `var`: the first entry at or after
/// the command line's name.
fn position_to(d: &mut UimDialog, list: &str, var: &str, n: u32, position: &mut String) {
    if n == 17 {
        *position = d.get(CMDLINE_VAR).trim().to_ascii_uppercase();
        d.set(CMDLINE_VAR, "");
    }
    if position.is_empty() {
        d.message = Some(("Type a name on the command line, then F17.".into(), true));
        return;
    }
    let l = d.list_mut(list);
    match l.rows.iter().position(|row| {
        row.iter()
            .any(|(k, v)| k == var && !v.is_empty() && *v >= *position)
    }) {
        Some(i) => l.top = i,
        None => d.message = Some((format!("No entry is at or after {position}."), true)),
    }
}

const LIBS: &str = "LIBS";

/// `WRKLIB LIB(name)`: the libraries a name selects, a blank first
/// entry for option 1.
pub fn wrklib(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let name = parm(r, "LIB").unwrap_or_else(|| "*LIBL".into());
    let mut d = dialog(host, "QUIWRKLIB")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let l = host.attrs().libl;
        let names: Vec<String> = match name.as_str() {
            "*LIBL" => {
                let mut v = l.sys.clone();
                v.extend(l.prd.iter().filter(|n| !n.is_empty()).cloned());
                v.extend(l.cur.iter().cloned());
                v.extend(l.usr.iter().cloned());
                v
            }
            "*USRLIBL" => l.cur.iter().cloned().chain(l.usr.iter().cloned()).collect(),
            "*CURLIB" => l.cur.iter().cloned().collect(),
            _ => Vec::new(),
        };
        let objs = if names.is_empty() {
            let generic = if name == "*ALL" || name == "*ALLUSR" {
                "*ALL".to_string()
            } else {
                name.clone()
            };
            let mut v = host
                .link
                .borrow_mut()
                .with(|c| c.list_objects("QSYS".into(), generic, "*LIB".into()));
            if name == "*ALLUSR" {
                v.retain(|o| !o.name.starts_with('Q') || o.name == "QGPL");
            }
            v
        } else {
            names
                .iter()
                .flat_map(|n| {
                    host.link
                        .borrow_mut()
                        .with(|c| c.list_objects("QSYS".into(), n.clone(), "*LIB".into()))
                })
                .collect()
        };
        let mut rows = vec![Vec::new()];
        rows.extend(objs.into_iter().map(|o| {
            vec![
                ("LIB".to_string(), o.name),
                ("LIBATR".to_string(), o.attr),
                ("ASPDEV".to_string(), String::new()),
                ("LIBTEXT".to_string(), o.text),
            ]
        }));
        fill(d, LIBS, rows);
        Ok(())
    };
    let mut position = String::new();
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            if let Some(c) = choice(&resp).filter(|c| c.option == "1") {
                let typed = d.get(CMDLINE_VAR).trim().to_ascii_uppercase();
                d.set(CMDLINE_VAR, "");
                let lib = cell(d, &c.list, c.row, "LIB");
                let lib = if lib.is_empty() { typed } else { lib };
                let cmd = if lib.is_empty() {
                    "?CRTLIB".to_string()
                } else {
                    format!("?CRTLIB LIB({lib})")
                };
                exec(job, host, d, &cmd);
                return Ok(Step::Reload);
            }
            Ok(match resp {
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    Step::Stay
                }
                Response::Return(n @ (16 | 17)) => {
                    position_to(d, LIBS, "LIB", n, &mut position);
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(job, host, surface, &mut d, "WRKLIB", &mut reload, &mut keys)
}

/// `WRKOBJLCK OBJ(lib/name) OBJTYPE()`.
pub fn wrkobjlck(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let obj = qual(r, "OBJ", "*LIBL");
    let ty = parm(r, "OBJTYPE").unwrap_or_default();
    let mut d = dialog(host, "QGWCOLCK")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "LOCKS", fetch(host, "OBJLCK", &[&obj, &ty])?);
        Ok(())
    };
    let mut keys = |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(others(d, &resp))
    };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKOBJLCK",
        &mut reload,
        &mut keys,
    )
}

/// `WRKOBJPGP PGP(name)`.
pub fn wrkobjpgp(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let pgp = parm(r, "PGP").unwrap_or_default();
    let mut d = dialog(host, "QUIWRKOPGP")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "OBJS", fetch(host, "PGP", &[&pgp])?);
        Ok(())
    };
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            if let Some(c) = choice(&resp).filter(|c| c.option == "4") {
                let (n, l, t) = (
                    cell(d, &c.list, c.row, "OBJ"),
                    cell(d, &c.list, c.row, "OBJLIB"),
                    cell(d, &c.list, c.row, "OBJTYPE"),
                );
                exec(job, host, d, &format!("?DLTOBJ OBJ({l}/{n}) OBJTYPE({t})"));
                return Ok(Step::Reload);
            }
            Ok(match resp {
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKOBJPGP",
        &mut reload,
        &mut keys,
    )
}

/// `DSPOVR`: the merged overrides; F11 every one with its call level,
/// option 5 one in full.
pub fn dspovr(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIDSPOVR")?;
    load(&mut d, "OVRS", fetch(host, "OVR", &[])?);
    loop {
        match show(&mut d, surface, "DSPOVR")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(5) => load(&mut d, "OVRS", fetch(host, "OVR", &[])?),
            Response::Return(11) => {
                let merged = d.get("MERGED") == "1";
                d.set("MERGED", if merged { "0" } else { "1" });
                d.view = usize::from(merged);
            }
            Response::Options(choices) => {
                for c in &choices {
                    if c.option == "5" || c.option == "8" {
                        let mut one = dialog(host, "QUIDSPOVR")?;
                        for var in ["FILE", "TYPE", "LEVEL", "KWDS"] {
                            one.set(var, &cell(&d, &c.list, c.row, var));
                        }
                        let mut reload = |_: &Host, _: &mut UimDialog| Ok(());
                        if display(host, surface, &mut one, "DSPOVRD", &mut reload).is_err() {
                            break;
                        }
                    }
                }
                d.list_mut("OVRS").options.clear();
            }
            _ => {}
        }
    }
}

/// `DSPSAVF FILE(lib/name)`: the panel of a function to come.
pub fn dspsavf(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let q = qual(r, "FILE", "*LIBL");
    let (lib, name) = q.split_once('/').unwrap_or(("*LIBL", &q));
    let mut d = dialog(host, "QUIDSPSAVF")?;
    d.set("SAVF", name);
    d.set("SAVFLIB", lib);
    d.set("OBJS", "0");
    fill(&mut d, "OBJS", Vec::new());
    d.message = Some(("Save files come in a later release.".into(), false));
    let mut reload = |_: &Host, _: &mut UimDialog| Ok(());
    display(host, surface, &mut d, "DSPSAVF", &mut reload)
}

/// `WRKPRB`: the problem log, empty in this release.
pub fn wrkprb(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGSXPNLS")?;
    fill(&mut d, "PRBS", Vec::new());
    let mut reload = |_: &Host, _: &mut UimDialog| Ok(());
    display(host, surface, &mut d, "WRKPRB", &mut reload)
}
