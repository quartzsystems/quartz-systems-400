// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service displays (`docs/SERVICE.md`): `WRKPRB` and its options,
//! `DSPPRB`'s details, Work with Problem, Enter text, `ANZPRB`'s guided
//! analysis (`QGSXPNLS`); `WRKCNTINF`, `WRKSRVPVD`, `DSPSRVSTS`
//! (`QESCNTINF`). Changes go through `service_op`, data through
//! `panel2_data`.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Step, cell, choice, dialog, display, exec, fetch, fill, load, no_more, parm, refused, show, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// A service action's answer lines.
fn op(host: &Host, what: &str, args: &[&str]) -> Result<Vec<String>, Fail> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    host.link
        .borrow_mut()
        .with(|l| l.service_op(what.into(), args))
        .map_err(|e| refused(&e))
}

/// A service action's first line on the message line, or its refusal.
fn act(host: &Host, d: &mut UimDialog, what: &str, args: &[&str]) {
    match op(host, what, args) {
        Ok(lines) => {
            if let Some(l) = lines.first() {
                d.message = Some((l.clone(), false));
            }
        }
        Err(f) => d.message = Some((f.text, true)),
    }
}

/// The statuses `STATUS` names, `*ALL` dropped.
fn statuses(r: &Resolved) -> Vec<String> {
    r.get("STATUS")
        .and_then(|b| b.value.as_ref())
        .map(|v| {
            v.items()
                .iter()
                .map(|i| i.source().trim().to_ascii_uppercase())
                .filter(|s| s != "*ALL" && !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `WRKPRB`, and `DSPPRB` of more than one problem.
pub fn wrkprb(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    if r.name == "DSPPRB"
        && let Some(id) = parm(r, "PRBID").filter(|i| !i.starts_with('*'))
    {
        return dspprb(host, surface, &id);
    }
    let status = statuses(r);
    let only = parm(r, "PRBID").filter(|i| !i.starts_with('*'));
    let mut d = dialog(host, "QGSXPNLS")?;
    let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let args: Vec<&str> = status.iter().map(String::as_str).collect();
        let (vars, mut rows) = fetch(h, "WRKPRB", &args)?;
        if let Some(id) = &only {
            rows.retain(|row| row.iter().any(|(k, v)| k == "PRBID" && v == id));
        }
        load(d, "PRBS", (vars, rows));
        Ok(())
    };
    let mut keys = |job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, resp: Response| {
        if let Response::Return(16) = resp {
            let ids: Vec<String> = (0..d.list_mut("PRBS").rows.len())
                .filter(|i| cell(d, "PRBS", *i, "PRBSTS") == "PREPARED")
                .map(|i| cell(d, "PRBS", i, "PRBID"))
                .collect();
            if ids.is_empty() {
                d.message = Some(("No problem is prepared for reporting.".into(), false));
            }
            for id in ids {
                act(host, d, "prb-report", &[&id]);
            }
            return Ok(Step::Reload);
        }
        let Some(c) = choice(&resp).cloned() else {
            return Ok(super::others(d, &resp));
        };
        let id = cell(d, "PRBS", c.row, "PRBID");
        match c.option.trim() {
            "2" => exec(job, host, d, &format!("?CHGPRB PRBID({id})")),
            "4" => exec(job, host, d, &format!("DLTPRB PRBID({id})")),
            "5" => {
                dspprb(host, surface, &id)?;
            }
            "6" => exec(job, host, d, &format!("DSPPRB PRBID({id}) OUTPUT(*PRINT)")),
            "8" => work_with(host, surface, &id)?,
            "9" => {
                d.message = Some(("Work with alerts is not available in this release.".into(), true));
            }
            "12" => note(host, surface, &id)?,
            _ => return Ok(no_more(d, &resp)),
        }
        Ok(Step::Reload)
    };
    work(job, host, surface, &mut d, "WRKPRB", &mut reload, &mut keys)
}

/// Fill a dialog with one problem's details and notes.
fn problem(host: &Host, d: &mut UimDialog, id: &str) -> Result<(), Fail> {
    let data = fetch(host, "DSPPRB", &[id])?;
    load(d, "NOTES", data);
    Ok(())
}

/// `DSPPRB PRBID(id)` at a display, and `WRKPRB` option 5.
pub fn dspprb(host: &Host, surface: &mut Surface, id: &str) -> Result<String, Fail> {
    let mut d = dialog(host, "QGSXPNLS")?;
    let id = id.to_string();
    let mut reload = |h: &Host, d: &mut UimDialog| problem(h, d, &id);
    display(host, surface, &mut d, "DSPPRB", &mut reload)
}

/// Work with Problem (`WRKPRB` option 8).
fn work_with(host: &Host, surface: &mut Surface, id: &str) -> Result<(), Fail> {
    let mut d = dialog(host, "QGSXPNLS")?;
    loop {
        problem(host, &mut d, id)?;
        d.set("OPTION", "");
        match show(&mut d, surface, "WRKPRBW")? {
            Response::Return(101) => act(host, &mut d, "prb-ready", &[id]),
            Response::Return(102) => act(host, &mut d, "prb-report", &[id]),
            Response::Return(103) => {
                dspprb(host, surface, id)?;
            }
            Response::Return(104) => note(host, surface, id)?,
            Response::Return(105) => act(host, &mut d, "prb-close", &[id]),
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            Response::Return(_) => {
                d.message = Some(("Type an option number, press Enter.".into(), true));
            }
            other => {
                no_more(&mut d, &other);
            }
        }
    }
}

/// Enter text about a problem (`WRKPRB` option 12).
fn note(host: &Host, surface: &mut Surface, id: &str) -> Result<(), Fail> {
    let mut d = dialog(host, "QGSXPNLS")?;
    problem(host, &mut d, id)?;
    d.set("NEWNOTE", "");
    loop {
        match show(&mut d, surface, "PRBNOTE")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            Response::Return(_) => {
                let text = d.get("NEWNOTE").trim().to_string();
                match op(host, "prb-note", &[id, &text]) {
                    Ok(_) => return Ok(()),
                    Err(f) => d.message = Some((f.text, true)),
                }
            }
            other => {
                no_more(&mut d, &other);
            }
        }
    }
}

/// `ANZPRB`: the area, then what identifies the problem, then the
/// problem logged.
pub fn anzprb(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGSXPNLS")?;
    loop {
        d.set("OPTION", "");
        let area = match show(&mut d, surface, "ANZPRB1")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(n @ 201..=205) => n - 200,
            Response::Return(_) => {
                d.message = Some(("Type an option number, press Enter.".into(), true));
                continue;
            }
            other => {
                no_more(&mut d, &other);
                continue;
            }
        };
        for v in ["ANZMSGID", "ANZOBJ", "ANZTEXT"] {
            d.set(v, "");
        }
        d.set("ANZSTOP", "N");
        loop {
            match show(&mut d, surface, "ANZPRB2")? {
                Response::Exit | Response::Gone => return Ok(String::new()),
                Response::Cancel => break,
                Response::Return(_) => {
                    let area = area.to_string();
                    let (m, o, s, t) = (d.get("ANZMSGID"), d.get("ANZOBJ"), d.get("ANZSTOP"), d.get("ANZTEXT"));
                    match op(host, "prb-analyze", &[&area, &m, &o, &s, &t]) {
                        Ok(lines) => {
                            let id = lines.first().cloned().unwrap_or_default();
                            d.set("PRBID", &id);
                            d.set("PRBSTS", "READY");
                            d.set("PRBSYMP", lines.get(1).map_or("", String::as_str));
                            let _ = show(&mut d, surface, "ANZPRB3")?;
                            return Ok(format!("Problem {id} created.\n"));
                        }
                        Err(f) => d.message = Some((f.text, true)),
                    }
                }
                other => {
                    no_more(&mut d, &other);
                }
            }
        }
    }
}

/// `WRKCNTINF`.
pub fn wrkcntinf(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QESCNTINF")?;
    loop {
        d.set("OPTION", "");
        match show(&mut d, surface, "WRKCNTINF")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Command(text) => exec(job, host, &mut d, &text),
            Response::Return(102) => {
                let mut c = dialog(host, "QESCNTINF")?;
                let mut reload = |h: &Host, c: &mut UimDialog| {
                    let data = fetch(h, "CNTINF", &[])?;
                    load(c, "", data);
                    Ok(())
                };
                display(host, surface, &mut c, "DSPCNTINF", &mut reload)?;
            }
            Response::Return(_) => {
                d.message = Some(("Type an option number, press Enter.".into(), true));
            }
            other => {
                no_more(&mut d, &other);
            }
        }
    }
}

/// `WRKSRVPVD`.
pub fn wrksrvpvd(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let cnn = parm(r, "CNNTYPE").unwrap_or_else(|| "*ANY".into());
    let mut d = dialog(host, "QESCNTINF")?;
    let mut reload = |h: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let (_, mut rows) = fetch(h, "SRVPVD", &[])?;
        if cnn != "*ANY" {
            rows.retain(|row| row.iter().any(|(k, v)| k == "CNNTYPE" && *v == cnn));
        }
        fill(d, "PVDS", rows);
        Ok(())
    };
    let mut keys = |job: &mut Job, host: &Host, _s: &mut Surface, d: &mut UimDialog, resp: Response| {
        let Some(c) = choice(&resp).cloned() else {
            return Ok(super::others(d, &resp));
        };
        let cp = cell(d, "PVDS", c.row, "CPNAME");
        match c.option.trim() {
            "5" => {
                let line = format!(
                    "{cp}: network {}, connection {}, {}.",
                    cell(d, "PVDS", c.row, "NETID"),
                    cell(d, "PVDS", c.row, "CNNTYPE"),
                    cell(d, "PVDS", c.row, "PVDTEXT")
                );
                d.message = Some((line, false));
                Ok(Step::Stay)
            }
            "9" => {
                exec(job, host, d, &format!("SNDSRVRQS ACTION(*TEST) RMTCPNAME({cp})"));
                Ok(Step::Reload)
            }
            _ => Ok(no_more(d, &resp)),
        }
    };
    let _ = host;
    work(job, host, surface, &mut d, "WRKSRVPVD", &mut reload, &mut keys)
}

/// `DSPSRVSTS` at a display.
pub fn dspsrvsts(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let number = r
        .get("JOB")
        .and_then(|b| b.value.as_ref())
        .and_then(|v| match v {
            cmd::Value::Qual(p) => p.get(2).map(|n| n.source().trim().to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let mut d = dialog(host, "QESCNTINF")?;
    let mut reload = |h: &Host, d: &mut UimDialog| {
        let data = fetch(h, "SRVSTS", &[&number])?;
        load(d, "", data);
        Ok(())
    };
    display(host, surface, &mut d, "DSPSRVSTS", &mut reload)
}
