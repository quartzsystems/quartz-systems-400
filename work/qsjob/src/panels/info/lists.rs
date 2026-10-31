// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPIPLA` (`QUIIPLA`), `WRKMSGD` (`QUIDSPMSGD`'s work panel) and
//! `WRKRDBDIRE` (`QUIRDBDIR`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{NO_PRINT, act, dialog, exec, fetch, fill, load, qual, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response};

/// `DSPIPLA`: the attributes the next IPL uses; F5 reads them again.
pub fn dspipla(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIIPLA")?;
    loop {
        load(&mut d, fetch(host, "IPLA", &[])?);
        match show(&mut d, surface, "DSPIPLA")? {
            Response::Return(5) => {}
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => return Ok(String::new()),
        }
    }
}

/// `WRKMSGD MSGID() MSGF()`: a message file's descriptions with change,
/// remove and display.
pub fn wrkmsgd(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (msgf, lib) = qual(r, "MSGF");
    let real = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), msgf.clone(), "*MSGF".into()))
        .ok_or_else(|| Fail {
            msgid: "CPF2407".into(),
            data: cmd::error::data(&[(&msgf, 10), (&lib, 10)]),
            text: String::new(),
        })?
        .lib;
    let from = super::parm(r, "MSGID");
    let mut d = dialog(host, "QUIDSPMSGD")?;
    d.copyright = true;
    d.set("MSGF", &msgf);
    d.set("MSGFLIB", &real);
    let mut last = String::new();
    loop {
        let all = host
            .link
            .borrow_mut()
            .with(|l| l.msgf_list(msgf.clone(), real.clone()))
            .map_err(|e| crate::panels::failed(&e))?;
        let rows = all
            .into_iter()
            .filter(|(id, _)| from.is_empty() || from.starts_with('*') || id.as_str() >= from.as_str())
            .map(|(id, t)| vec![("ID".to_string(), id), ("IDTEXT".to_string(), t)])
            .collect();
        fill(&mut d, "IDS", rows);
        match show(&mut d, surface, "WORK")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                act(job, host, &mut d, &choices, &mut |d, c| {
                    d.message = Some((format!("Option {} is not valid here.", c.option.trim()), true));
                    Ok(())
                })?;
            }
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Print => d.message = Some((NO_PRINT.into(), true)),
            _ => {}
        }
    }
}

/// `WRKRDBDIRE RDB()`: the relational database directory; option 5
/// shows an entry's detail in the report viewer.
pub fn wrkrdbdire(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let which = super::parm(r, "RDB");
    let which = if which.is_empty() { "*ALL".to_string() } else { which };
    let mut d = dialog(host, "QUIRDBDIR")?;
    d.copyright = true;
    loop {
        let dir = crate::command::ddm::directory(host);
        let entries = dir.select(&which);
        let rows = entries
            .iter()
            .map(|e| {
                vec![
                    ("RDB".to_string(), e.rdb.clone()),
                    ("RMTLOC".to_string(), e.rmtloc.clone()),
                    ("RDBTEXT".to_string(), e.text.clone()),
                ]
            })
            .collect();
        fill(&mut d, "RDBS", rows);
        let pos = d.get("POSTO").trim().to_ascii_uppercase();
        if !pos.is_empty() {
            let at = entries.iter().position(|e| e.rdb >= pos);
            d.list_mut("RDBS").top = at.unwrap_or(0);
            d.set("POSTO", "");
        }
        match show(&mut d, surface, "WRKRDBDIRE")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                act(job, host, &mut d, &choices, &mut |d, c| {
                    if c.option.trim() == "6" {
                        d.message = Some((NO_PRINT.into(), true));
                        return Ok(());
                    }
                    let name = d
                        .row(&c.list, c.row)
                        .into_iter()
                        .find(|(v, _)| v == "RDB")
                        .map(|(_, x)| x.trim().to_string())
                        .unwrap_or_default();
                    let dir = crate::command::ddm::directory(host);
                    let Some(e) = dir.select(&name).into_iter().next() else {
                        return Ok(());
                    };
                    let rows = ddm::objects::panel::display_entry(e);
                    let body: Vec<String> = rows.iter().skip(2).take(19).map(|l| l.trim_end().to_string()).collect();
                    let title = rows.first().map_or("", |t| t.trim()).to_string();
                    crate::panels::system2::view_report(host, surface, &title, "QSYSPRT", &body.join("\n"))?;
                    Ok(())
                })?;
            }
            Response::Print | Response::Return(6) => d.message = Some((NO_PRINT.into(), true)),
            Response::Return(22) => {
                d.message = Some(("Every field shows whole in this release.".into(), false));
            }
            _ => {}
        }
    }
}
