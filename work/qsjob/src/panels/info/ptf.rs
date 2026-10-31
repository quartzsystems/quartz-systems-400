// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPPTF` and `WRKPTFGRP` over the PTF database (`QSYS/QSPTFDB`,
//! `lic/slic/src/cpp/ptfcmd.rs`), through the service's panel data.

use cmd::Resolved;
use mi::Job;

use super::{NO_PRINT, act, dialog, fetch, load, parm, parm_list, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

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
                        d.message = Some((format!("Option {} is not valid here.", c.option.trim()), true));
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
                d.message = Some(("This release has one view of the PTFs.".into(), false));
            }
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
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
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// A list entry's value.
fn cell(d: &crate::display::uim::UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x.trim().to_string())
        .unwrap_or_default()
}
