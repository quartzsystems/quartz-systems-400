// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media displays (`QSMEDIA`): `WRKIMGCLG` (F11 between the status
//! and text views, option 12 into the entries), `WRKIMGCLGE`,
//! `WRKTAPCTG` (options 6 and 8 mount and demount through `service_op`)
//! and `WRKMLBSTS` (options 4 to 6 set a drive's allocation, 8 its
//! description). The panels follow the 7.1 oracle's captures. The data
//! comes through `panel2_data`.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Step, cell, choice, dialog, exec, fetch, load, no_more, others, parm, refused, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// A library action through the service; its line (or refusal) on the
/// message line.
fn act(host: &Host, d: &mut UimDialog, what: &str, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    match host.link.borrow_mut().with(|l| l.service_op(what.into(), args)).map_err(|e| refused(&e)) {
        Ok(lines) => {
            if let Some(m) = lines.first().and_then(|l| crate::panels::said(l)) {
                d.send(m);
            }
        }
        Err(f) => d.send(crate::display::msgline::of_fail(host, &f)),
    }
}

/// `WRKIMGCLG`.
pub fn wrkimgclg(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let which = parm(r, "IMGCLG").unwrap_or_else(|| "*ALL".into());
    let ty = parm(r, "TYPE").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QSMEDIA")?;
    let mut text_view = false;
    loop {
        d.set("CLGVIEW", if text_view { "TEXT" } else { "STATUS" });
        let panel = if text_view { "WRKIMGCLG2" } else { "WRKIMGCLG" };
        let mut flip = false;
        {
            let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
                // Option 1 creates from the list's add entry (`EXTACT`).
                load(d, "CLGS", fetch(h, "IMGCLG", &[&which, &ty])?);
                Ok(())
            };
            let mut keys = |job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, resp: Response| {
                if let Response::Return(11) = resp {
                    flip = true;
                    return Ok(Step::Leave);
                }
                let Some(c) = choice(&resp).cloned() else {
                    return Ok(others(d, &resp));
                };
                let name = cell(d, "CLGS", c.row, "CLG");
                match c.option.trim() {
                    "12" if !name.is_empty() => {
                        wrkimgclge(job, host, surface, &name)?;
                        Ok(Step::Reload)
                    }
                    "2" => {
                        d.error("CPD9B9F", &[]);
                        Ok(Step::Stay)
                    }
                    _ => Ok(no_more(d, &resp)),
                }
            };
            work(job, host, surface, &mut d, panel, &mut reload, &mut keys)?;
        }
        if !flip {
            return Ok(String::new());
        }
        text_view = !text_view;
    }
}

/// `WRKIMGCLGE IMGCLG(name)`, and `WRKIMGCLG` option 12.
pub fn wrkimgclge(job: &mut Job, host: &Host, surface: &mut Surface, name: &str) -> Result<String, Fail> {
    let mut d = dialog(host, "QSMEDIA")?;
    let name = name.to_string();
    let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(h, "IMGCLGE", &[&name])?;
        load(d, "ENTS", data);
        Ok(())
    };
    // The add entry offers the next free index, as IBM's does.
    d.list_mut("ENTS").set_ext("IDX", "*AVAIL");
    let clg = name.clone();
    let mut keys = |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(match resp {
            Response::Return(6) => {
                exec(job, host, d, &format!("?LODIMGCLG IMGCLG({clg})"));
                Step::Reload
            }
            Response::Return(7) => {
                exec(job, host, d, &format!("?VFYIMGCLG IMGCLG({clg})"));
                Step::Reload
            }
            other => others(d, &other),
        })
    };
    work(job, host, surface, &mut d, "WRKIMGCLGE", &mut reload, &mut keys)
}

/// `WRKTAPCTG`.
pub fn wrktapctg(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let dev = parm(r, "DEV").unwrap_or_default();
    let cgy = parm(r, "CGY").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QSMEDIA")?;
    let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(h, "TAPCTG", &[&dev, &cgy])?;
        load(d, "CTGS", data);
        Ok(())
    };
    let lib = parm(r, "DEV").unwrap_or_default();
    let mut keys = |_: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        let Some(c) = choice(&resp).cloned() else {
            return Ok(others(d, &resp));
        };
        let ctg = cell(d, "CTGS", c.row, "CTG");
        match c.option.trim() {
            "6" => act(host, d, "mlb-mount", &[&lib, &ctg]),
            "8" => act(host, d, "mlb-demount", &[&lib, &ctg]),
            _ => return Ok(no_more(d, &resp)),
        }
        Ok(Step::Reload)
    };
    work(job, host, surface, &mut d, "WRKTAPCTG", &mut reload, &mut keys)
}

/// `WRKMLBSTS`.
pub fn wrkmlbsts(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let which = parm(r, "MLB").unwrap_or_else(|| "*ALL".into());
    let mut d = dialog(host, "QSMEDIA")?;
    let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(h, "MLBSTS", &[&which])?;
        load(d, "RSRCS", data);
        Ok(())
    };
    let mut keys = |_: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        let Some(c) = choice(&resp).cloned() else {
            if let Response::Return(23) = resp {
                d.info("CPI9BE9", &[]);
                return Ok(Step::Stay);
            }
            return Ok(others(d, &resp));
        };
        let rsrc = cell(d, "RSRCS", c.row, "RSRC");
        let lib = cell(d, "RSRCS", c.row, "RSRCLIB");
        let alloc = match c.option.trim() {
            "4" => "*ALLOCATED",
            "5" => "*UNPROTECTED",
            "6" => "*DEALLOCATED",
            _ => return Ok(no_more(d, &resp)),
        };
        // Drives are the indented rows.
        if !rsrc.starts_with(' ') {
            d.error("CPD9BA0", &[]);
            return Ok(Step::Stay);
        }
        act(host, d, "mlb-alloc", &[&lib, rsrc.trim(), alloc]);
        Ok(Step::Reload)
    };
    work(job, host, surface, &mut d, "WRKMLBSTS", &mut reload, &mut keys)
}
