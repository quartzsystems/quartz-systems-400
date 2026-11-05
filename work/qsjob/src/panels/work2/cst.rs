// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Physical file constraints (`QSDBCST`): `WRKPFCST`, whose options
//! run `CHGPFCST`, `RMVPFCST` and `DSPCPCST`, and `EDTCPCST`, where a
//! sequence number has a constraint in check pending verified again.

use cmd::Resolved;
use mi::Job;

use super::{Step, cell, dialog, exec, fetch, load, others, parm, qual, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// `WRKPFCST FILE(lib/file) CST(...) TYPE(...)`.
pub fn wrkpfcst(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let q = qual(r, "FILE", "*LIBL");
    let (lib, file) = q.split_once('/').map_or(("*LIBL".to_string(), q.clone()), |(l, f)| (l.to_string(), f.to_string()));
    let cst = parm(r, "CST").unwrap_or_else(|| "*ALL".into());
    let ty = parm(r, "TYPE").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QSDBCST")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "CSTS", fetch(host, "PFCST", &[&lib, &file, &cst, &ty])?);
        Ok(())
    };
    let mut keys = |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| Ok(others(d, &resp));
    work(job, host, surface, &mut d, "WRKPFCST", &mut reload, &mut keys)
}

/// `EDTCPCST`: Enter verifies, in sequence order, the constraints given
/// a number from 1 to 99; `*HLD` leaves one in check pending.
pub fn edtcpcst(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QSDBCST")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "CPCSTS", fetch(host, "CPCST", &[])?);
        Ok(())
    };
    let mut keys = |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(match resp {
            Response::Return(1) => {
                let n = d.list_mut("CPCSTS").rows.len();
                let mut order: Vec<(u32, String)> = Vec::new();
                for i in 0..n {
                    let seq = cell(d, "CPCSTS", i, "SEQ");
                    let seq = seq.trim();
                    if seq.eq_ignore_ascii_case("*HLD") || seq.is_empty() {
                        continue;
                    }
                    match seq.parse::<u32>() {
                        Ok(s) if (1..=99).contains(&s) => {
                            let command = format!(
                                "CHGPFCST FILE({}/{}) CST({}) STATE(*ENABLED)",
                                cell(d, "CPCSTS", i, "CPLIB").trim(),
                                cell(d, "CPCSTS", i, "CPFILE").trim(),
                                cell(d, "CPCSTS", i, "CPCST").trim()
                            );
                            order.push((s, command));
                        }
                        _ => {
                            d.error("CPD9B89", &[seq]);
                            return Ok(Step::Stay);
                        }
                    }
                }
                order.sort_by_key(|(s, _)| *s);
                for (_, command) in &order {
                    exec(job, host, d, command);
                }
                Step::Reload
            }
            other => others(d, &other),
        })
    };
    work(job, host, surface, &mut d, "EDTCPCST", &mut reload, &mut keys)
}
