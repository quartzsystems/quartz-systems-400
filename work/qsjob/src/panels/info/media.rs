// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media displays over the service's devices
//! (`lic/slic/src/cpp/meddsp.rs`, `QSMEDIA`): `DSPTAP` (labels, the
//! volume label alone or the saves), `DSPTAPCTG` (one cartridge to a
//! display; Enter shows the next), `DSPOPT` (the view `DATA` asks for)
//! and `WRKOPTVOL`.

use cmd::Resolved;
use mi::Job;

use super::{act, dialog, exec, fetch, load, parm, parm_list, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

/// Show a display panel until it is left.
fn until_left(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<String, Fail> {
    loop {
        match show(d, surface, panel)? {
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => return Ok(String::new()),
            Response::Print => d.error("CPD9B84", &[]),
            _ => {}
        }
    }
}

/// The `SEQNBR` range: `*FIRST` 1; `*LAST` (or nothing) the highest,
/// `*ONLY` the starting number.
fn seq_range(r: &Resolved) -> (u32, u32) {
    let s = parm_list(r, "SEQNBR");
    let first = match s.first().map(String::as_str) {
        None | Some("" | "*FIRST") => 1,
        Some(n) => n.parse().unwrap_or(1),
    };
    let last = match s.get(1).map(String::as_str) {
        None | Some("" | "*LAST") => 16_777_215,
        Some("*ONLY") => first,
        Some(n) => n.parse().unwrap_or(first),
    };
    (first, last)
}

/// `DSPTAP DEV VOL LABEL SEQNBR DATA ENDOPT`.
pub fn dsptap(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (first, last) = seq_range(r);
    let data = match parm(r, "DATA").as_str() {
        "" => "*LABELS".to_string(),
        d => d.to_string(),
    };
    let args = [
        parm(r, "DEV"),
        parm(r, "VOL"),
        parm(r, "LABEL"),
        first.to_string(),
        last.to_string(),
        data.clone(),
        parm(r, "ENDOPT"),
    ];
    let mut d = dialog(host, "QSMEDIA")?;
    load(&mut d, fetch(host, "TAP", &args)?);
    let panel = match data.as_str() {
        "*VOLLBL" => "DSPTAPVOL",
        "*SAVRST" => "DSPTAPSAV",
        _ => "DSPTAP",
    };
    until_left(&mut d, surface, panel)
}

/// `DSPTAPCTG DEV CTG CGY`: each cartridge in turn.
pub fn dsptapctg(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let ctgs = parm_list(r, "CTG").join(",");
    let (_, lists) = fetch(host, "TAPCTG", &[parm(r, "DEV"), ctgs, parm(r, "CGY")])?;
    let rows = lists.into_iter().find(|(n, _)| n == "CTGS").map(|(_, rows)| rows).unwrap_or_default();
    let mut d = dialog(host, "QSMEDIA")?;
    for row in &rows {
        for (k, v) in row {
            d.set(k, v);
        }
        match show(&mut d, surface, "DSPTAPCTG")? {
            Response::Exit | Response::Cancel | Response::Gone => break,
            _ => {}
        }
    }
    Ok(String::new())
}

/// `DSPOPT VOL DEV DATA PATH`.
pub fn dspopt(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let path = r
        .get("PATH")
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().trim().trim_matches('\'').to_string())
        .unwrap_or_default();
    let args = [parm(r, "VOL"), parm(r, "DEV"), parm(r, "DATA"), path];
    let mut d = dialog(host, "QSMEDIA")?;
    load(&mut d, fetch(host, "OPT", &args)?);
    let panel = match d.get("VIEW").trim() {
        "DIRATR" => "DSPOPTDIR",
        "FILATR" => "DSPOPTFIL",
        "SAVRST" => "DSPOPTSAV",
        _ => "DSPOPTVOL",
    };
    until_left(&mut d, surface, panel)
}

/// `WRKOPTVOL DEV VOL CSI`: the volumes, read again on F5 and when the
/// device asked for changes. Option 1 on the add entry above the list
/// prompts `ADDOPTCTG` for the volume typed beside it, as the 7.1
/// oracle's does (`audit/displays/wrkoptvol-add-prompt71`).
pub fn wrkoptvol(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let vol = parm(r, "VOL");
    let csi = parm(r, "CSI");
    let mut dev = parm(r, "DEV");
    let mut d = dialog(host, "QSMEDIA")?;
    let mut last = String::new();
    let mut read = true;
    loop {
        if read {
            load(&mut d, fetch(host, "OPTVOLS", &[dev.clone(), vol.clone(), csi.clone()])?);
            read = false;
        }
        match show(&mut d, surface, "WRKOPTVOL")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                act(job, host, &mut d, &choices, &mut |d, c| {
                    d.error("CPD9B89", &[c.option.trim()]);
                    Ok(())
                })?;
                read = true;
            }
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.set_message(exec(job, host, &text));
                d.set(CMDLINE_VAR, "");
                read = true;
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(5) => read = true,
            Response::Return(0) => {
                let typed = d.get("WDEV").trim().to_ascii_uppercase();
                if typed != dev {
                    dev = typed;
                    read = true;
                }
            }
            Response::Print | Response::Return(6) => d.error("CPD9B84", &[]),
            Response::Return(4) => d.error("CPD9B86", &[]),
            Response::Return(11 | 14) => {
                d.error("CPD9B8B", &[]);
            }
            Response::Return(23) => d.info("CPI9BE9", &[]),
            Response::Return(_) | Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}
