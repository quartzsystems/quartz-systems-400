// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKSBMJOB` (`QGWCSBSJ`) and `WRKSBSD` (`QUIWRKSBSD`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Step, cell, choice, dialog, exec, fetch, fill, load, others, qual, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

/// `WRKSBMJOB`: the jobs this user submitted.
pub fn wrksbmjob(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCSBSJ")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "JOBS", fetch(host, "SBMJOB", &[])?);
        d.set("*DATETIME", &crate::panels::work::stamp(host));
        Ok(())
    };
    let mut keys = |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(match resp {
            Response::Return(11) => {
                d.error("CPD9B81", &[]);
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
        "WRKSBMJOB",
        &mut reload,
        &mut keys,
    )
}

const SBSDS: &str = "SBSDS";

/// `WRKSBSD SBSD(lib/name)`: the descriptions a name selects; option 1
/// on the add entry creates one; F17 positions the list at the command
/// line's name, F16 again.
pub fn wrksbsd(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let q = qual(r, "SBSD", "*LIBL");
    let (lib, name) = q.split_once('/').unwrap_or(("*LIBL", &q));
    let (lib, name) = (lib.to_string(), name.to_string());
    let mut d = dialog(host, "QUIWRKSBSD")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let libs: Vec<String> = match lib.as_str() {
            "*LIBL" | "*USRLIBL" | "*ALLUSR" | "*ALL" | "*CURLIB" => {
                let l = host.attrs().libl;
                let mut v: Vec<String> = l.sys.clone();
                v.extend(l.prd.iter().filter(|n| !n.is_empty()).cloned());
                v.extend(l.cur.iter().cloned());
                v.extend(l.usr.iter().cloned());
                v
            }
            other => vec![other.to_string()],
        };
        let mut rows = Vec::new();
        for l in libs {
            let objs = host
                .link
                .borrow_mut()
                .with(|c| c.list_objects(l.clone(), name.clone(), "*SBSD".into()));
            for o in objs {
                rows.push(vec![
                    ("SBSD".to_string(), o.name),
                    ("SBSDLIB".to_string(), o.lib),
                    ("SBSDTEXT".to_string(), o.text),
                ]);
            }
        }
        fill(d, SBSDS, rows);
        Ok(())
    };
    let mut position = String::new();
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            // Option 1 on the add entry: its name and library, else the
            // command line's name.
            if let Some(c) = choice(&resp).filter(|c| c.option == "1").cloned() {
                let mut sbsd = cell(d, &c.list, c.row, "SBSD").trim().to_ascii_uppercase();
                if sbsd.is_empty() {
                    sbsd = d.get(CMDLINE_VAR).trim().to_ascii_uppercase();
                    d.set(CMDLINE_VAR, "");
                }
                let lib = cell(d, &c.list, c.row, "SBSDLIB").trim().to_ascii_uppercase();
                let cmd = match (sbsd.is_empty(), lib.is_empty()) {
                    (true, _) => "?CRTSBSD".to_string(),
                    (false, true) => format!("?CRTSBSD SBSD({sbsd})"),
                    (false, false) => format!("?CRTSBSD SBSD({lib}/{sbsd})"),
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
                    if n == 17 {
                        position = d.get(CMDLINE_VAR).trim().to_ascii_uppercase();
                        d.set(CMDLINE_VAR, "");
                    }
                    let l = d.list_mut(SBSDS);
                    match l
                        .rows
                        .iter()
                        .position(|row| row.iter().any(|(k, v)| k == "SBSD" && *v >= position))
                    {
                        Some(i) if !position.is_empty() => l.top = i,
                        _ => {
                            d.error("CPD9B87", &[]);
                        }
                    }
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
        "WRKSBSD",
        &mut reload,
        &mut keys,
    )
}
