// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPIPLA` (`QUIIPLA`), `WRKMSGD` (`QUIDSPMSGD`'s work panel),
//! `WRKRDBDIRE` (`QUIRDBDIR`) and `WRKDDMF` (`QUIWRKDDMF`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{act, dialog, exec, fetch, fill, load, qual, show};
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
            Response::Print => d.error("CPD9B84", &[]),
            _ => return Ok(String::new()),
        }
    }
}

/// `WRKMSGD MSGID() MSGF()`: a message file's descriptions with change,
/// remove and display.
pub fn wrkmsgd(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
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
        let ids: Vec<(String, String)> = all
            .into_iter()
            .filter(|(id, _)| {
                from.is_empty() || from.starts_with('*') || id.as_str() >= from.as_str()
            })
            .collect();
        // `Position to`: the list starts at the first identifier from it.
        let pos = d.get("POSTO").trim().to_ascii_uppercase();
        let at = ids
            .iter()
            .position(|(id, _)| !pos.is_empty() && id.as_str() >= pos.as_str());
        let rows = ids
            .into_iter()
            .map(|(id, t)| vec![("WID".to_string(), id), ("IDTEXT".to_string(), t)])
            .collect();
        fill(&mut d, "WIDS", rows);
        if !pos.is_empty() {
            d.list_mut("WIDS").top = at.unwrap_or(0);
            d.set("POSTO", "");
        }
        match show(&mut d, surface, "WORK")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                act(job, host, &mut d, &choices, &mut |d, c| {
                    d.error(
                        if c.option.trim() == "6" {
                            "CPD9B84"
                        } else {
                            "CPD6A7C"
                        },
                        &[],
                    );
                    Ok(())
                })?;
            }
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.set_message(exec(job, host, &text));
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(6) => {
                d.set_message(exec(job, host, &format!("?ADDMSGD MSGF({real}/{msgf})")));
            }
            Response::Print => d.error("CPD9B84", &[]),
            _ => {}
        }
    }
}

/// `WRKRDBDIRE RDB()`: the relational database directory; option 5
/// shows an entry's detail in the report viewer.
pub fn wrkrdbdire(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let which = super::parm(r, "RDB");
    let which = if which.is_empty() {
        "*ALL".to_string()
    } else {
        which
    };
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
                        d.error("CPD9B84", &[]);
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
                    let body: Vec<String> = rows
                        .iter()
                        .skip(2)
                        .take(19)
                        .map(|l| l.trim_end().to_string())
                        .collect();
                    let title = rows.first().map_or("", |t| t.trim()).to_string();
                    crate::panels::system2::view_report(
                        host,
                        surface,
                        &title,
                        "QSYSPRT",
                        &body.join("\n"),
                    )?;
                    Ok(())
                })?;
            }
            Response::Print | Response::Return(6) => d.error("CPD9B84", &[]),
            Response::Return(22) => {
                d.info("CPI9B81", &[]);
            }
            _ => {}
        }
    }
}

/// `WRKDDMF FILE()`: the DDM files the name selects; options 5 and 6
/// run `DSPFD`, F22 shows every remote file and location whole.
pub fn wrkddmf(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let (name, lib) = qual(r, "FILE");
    let mut d = dialog(host, "QUIWRKDDMF")?;
    d.copyright = true;
    loop {
        let files = crate::command::ddm::ddm_files(&host.link, &lib, &name);
        let location = |f: &ddm::objects::DdmFile| {
            if f.rmtloc == "*RDB" {
                f.rdb.trim_matches('\'').to_string()
            } else {
                f.rmtloc.trim_matches('\'').to_string()
            }
        };
        let rows = files
            .iter()
            .map(|(l, f)| {
                vec![
                    ("LCLFILE".to_string(), format!("{l}/{}", f.file)),
                    ("RMTFILE".to_string(), f.rmtfile.clone()),
                    ("RMTLOC".to_string(), location(f)),
                ]
            })
            .collect();
        fill(&mut d, "FILES", rows);
        let pos = d.get("POSTO").trim().to_ascii_uppercase();
        if !pos.is_empty() {
            let at = files.iter().position(|(_, f)| f.file >= pos);
            d.list_mut("FILES").top = at.unwrap_or(0);
            d.set("POSTO", "");
        }
        match show(&mut d, surface, "WRKDDMF")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => act(job, host, &mut d, &choices, &mut |_, _| Ok(()))?,
            Response::Print | Response::Return(9) => d.error("CPD9B84", &[]),
            Response::Return(22) => {
                let body: Vec<String> = files
                    .iter()
                    .map(|(l, f)| format!("{l}/{}  {}  {}", f.file, f.rmtfile, location(f)))
                    .collect();
                // The report's title is the panel's.
                let title = d
                    .group
                    .panel("WRKDDMF")
                    .map(|p| uim::model::Span::plain(&p.title))
                    .unwrap_or_default();
                crate::panels::system2::view_report(
                    host,
                    surface,
                    &title,
                    "QSYSPRT",
                    &body.join("\n"),
                )?;
            }
            _ => {}
        }
    }
}
