// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPPTF`, `WRKPTFGRP` and `DSPPTFCVR` over the PTF database (`QSYS/QSPTFDB`,
//! `lic/slic/src/cpp/ptfcmd.rs`), through the service's panel data.

use cmd::Resolved;
use mi::Job;

use super::{act, dialog, fetch, load, parm, parm_list, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// `DSPPTF LICPGM() SELECT()`: the product's PTFs; option 5 shows what
/// the database holds for one.
pub fn dspptf(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let product = parm(r, "LICPGM");
    let select = parm_list(r, "SELECT").join(",");
    let mut d = dialog(host, "QUIDSPPTF")?;
    loop {
        load(&mut d, fetch(host, "PTF", &[product.clone(), select.clone()])?);
        match show(&mut d, surface, "DSPPTF")? {
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(0) => {
                return Ok(String::new());
            }
            Response::Options(choices) => {
                // F3 on a detail leaves the command (`CPF6801` here).
                let done = act(job, host, &mut d, &choices, &mut |d, c| {
                    if c.option.trim() != "5" {
                        d.error("CPD6A7C", &[]);
                        return Ok(());
                    }
                    let id = cell(d, &c.list, c.row, "PTFID");
                    let detail = fetch(host, "PTFD", &[id])?;
                    let mut p = dialog(host, "QUIDSPPTF")?;
                    load(&mut p, detail);
                    if matches!(show(&mut p, surface, "DSPPTFD")?, Response::Exit | Response::Gone) {
                        return Err(Fail::new("CPF6801", ""));
                    }
                    Ok(())
                });
                match done {
                    Ok(()) => {}
                    Err(f) if f.msgid == "CPF6801" => return Ok(String::new()),
                    Err(f) => return Err(f),
                }
            }
            Response::Return(11) => {
                d.error("CPD9B8B", &[]);
            }
            Response::Print => d.error("CPD9B84", &[]),
            _ => {}
        }
    }
}

/// `WRKPTFGRP PTFGRP()`: the PTF groups, their levels and statuses.
pub fn wrkptfgrp(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let pattern = parm(r, "PTFGRP");
    let mut d = dialog(host, "QUIDSPPTF")?;
    loop {
        load(&mut d, fetch(host, "PTFGRP", std::slice::from_ref(&pattern))?);
        match show(&mut d, surface, "WRKPTFGRP")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Print => d.error("CPD9B84", &[]),
            _ => {}
        }
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

/// `DSPPTFCVR LICPGM() SELECT() CVRATR() STATUS()`: the cover letters
/// that match, one shown at once when only one does; option 5 shows a
/// letter, option 6 prints it.
pub fn dspptfcvr(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let args = [
        parm(r, "LICPGM"),
        parm_list(r, "SELECT").join(","),
        parm_list(r, "CVRATR").join(","),
        parm(r, "STATUS"),
    ];
    let mut d = dialog(host, "QUIDSPPTF")?;
    load(&mut d, fetch(host, "PTFCVR", &args)?);
    if d.list_mut("CVRS").rows.len() == 1 {
        letter(host, surface, &d, 0)?;
        return Ok(String::new());
    }
    loop {
        match show(&mut d, surface, "SELCVR")? {
            Response::Exit | Response::Gone | Response::Cancel | Response::Return(0) => return Ok(String::new()),
            Response::Options(choices) => {
                for c in &choices {
                    match c.option.trim() {
                        "5" => {
                            if letter(host, surface, &d, c.row)? {
                                return Ok(String::new());
                            }
                        }
                        "6" => {
                            let line = format!(
                                "DSPPTFCVR LICPGM({}) SELECT({}) OUTPUT(*PRINT)",
                                cell(&d, &c.list, c.row, "PRODID"),
                                cell(&d, &c.list, c.row, "PTFID")
                            );
                            let out = crate::command::run(job, host, &line);
                            d.message = Some(match out.escape {
                                Some(f) => (f.text.lines().last().unwrap_or("").trim().to_string(), true),
                                None => (out.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string(), false),
                            });
                        }
                        o => d.message = Some((format!("Option {o} is not valid here."), true)),
                    }
                }
                d.list_mut("CVRS").options.clear();
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// One cover letter of the list; whether F3 asked to leave the command.
fn letter(host: &Host, surface: &mut Surface, list: &UimDialog, row: usize) -> Result<bool, Fail> {
    let mut p = dialog(host, "QUIDSPPTF")?;
    for v in ["PTFID", "PRODID", "PTFRLS"] {
        p.set(v, &cell(list, "CVRS", row, v));
    }
    let text = list
        .row("CVRS", row)
        .into_iter()
        .find(|(v, _)| v == "LETTER")
        .map(|(_, x)| x)
        .unwrap_or_default();
    p.clear_list("LINES");
    for l in text.lines() {
        p.add_row("LINES", vec![("CVRLINE".to_string(), l.to_string())]);
    }
    p.list_mut("LINES").complete = true;
    loop {
        match show(&mut p, surface, "DSPCVR")? {
            Response::Exit | Response::Gone => return Ok(true),
            Response::Cancel | Response::Return(_) => return Ok(false),
            _ => {}
        }
    }
}
