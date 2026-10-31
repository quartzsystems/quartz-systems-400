// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPBCKSTS` over the backup history (`QUSRSYS/QEZBCKHST`,
//! `lic/slic/src/cpp/opacmd.rs`): the last backup of each item
//! (`QEZBCKUP` `DSPBCKSTS`), F22 the runs (`DSPBCKHST`), option 5 one
//! run's saves (`DSPBCKDTL`).

use super::{datetime, dialog, fetch, load, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// `DSPBCKSTS`.
pub fn dspbcksts(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QEZBCKUP")?;
    load(&mut d, fetch(host, "BCKSTS", &[])?);
    loop {
        d.set("*DATETIME", &datetime(host));
        match show(&mut d, surface, "DSPBCKSTS")? {
            Response::Return(22) => {
                if history(host, surface, &mut d)? {
                    return Ok(String::new());
                }
            }
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => return Ok(String::new()),
            _ => {}
        }
    }
}

/// F22: the runs; option 5 shows one's saves. Whether F3 asked to leave.
fn history(host: &Host, surface: &mut Surface, d: &mut UimDialog) -> Result<bool, Fail> {
    loop {
        d.set("*DATETIME", &datetime(host));
        match show(d, surface, "DSPBCKHST")? {
            Response::Exit | Response::Gone => return Ok(true),
            Response::Options(choices) => {
                for c in &choices {
                    if c.option.trim() != "5" {
                        d.error("CPD6A7C", &[c.option.trim()]);
                        continue;
                    }
                    let row = d.row("HIST", c.row);
                    let mut p = dialog(host, "QEZBCKUP")?;
                    for (k, v) in &row {
                        p.set(k, v);
                    }
                    p.clear_list("STEPS");
                    let steps = row.iter().find(|(k, _)| k == "HSTEPS").map(|(_, v)| v.clone()).unwrap_or_default();
                    for s in steps.split(';').filter(|s| !s.is_empty()) {
                        p.add_row("STEPS", vec![("HSTEP".to_string(), s.to_string())]);
                    }
                    p.list_mut("STEPS").complete = true;
                    p.set("*DATETIME", &datetime(host));
                    if matches!(show(&mut p, surface, "DSPBCKDTL")?, Response::Exit | Response::Gone) {
                        return Ok(true);
                    }
                }
                d.list_mut("HIST").options.clear();
            }
            Response::Cancel | Response::Return(_) => return Ok(false),
            _ => {}
        }
    }
}
