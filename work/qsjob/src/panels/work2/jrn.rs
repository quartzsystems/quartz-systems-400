// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journal displays: `WRKJRN` (`QGJOWKMN`), `WRKJRNA` with its receiver
//! directory and journaled objects (`QGJOJRNA`), `DSPJRN` (`QGJOJRNE`)
//! and `DSPJRNRCVA` (`QGJORCV`).

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Step, choice, dialog, display, fetch, fill, load, no_more, others, qual, show, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// `WRKJRN JRN(lib/name)`: the journals a name selects.
pub fn wrkjrn(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let q = qual(r, "JRN", "*LIBL");
    let (lib, name) = q.split_once('/').unwrap_or(("*LIBL", &q));
    let (lib, name) = (
        lib.to_string(),
        if name.is_empty() {
            "*ALL".to_string()
        } else {
            name.to_string()
        },
    );
    let mut d = dialog(host, "QGJOWKMN")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let libs: Vec<String> = match lib.as_str() {
            "*LIBL" | "*USRLIBL" | "*CURLIB" | "*ALL" | "*ALLUSR" => {
                let l = host.attrs().libl;
                match lib.as_str() {
                    "*CURLIB" => l.cur.iter().cloned().collect(),
                    "*USRLIBL" => l.cur.iter().cloned().chain(l.usr.iter().cloned()).collect(),
                    _ => {
                        let mut v = l.sys.clone();
                        v.extend(l.prd.iter().filter(|n| !n.is_empty()).cloned());
                        v.extend(l.cur.iter().cloned());
                        v.extend(l.usr.iter().cloned());
                        v
                    }
                }
            }
            other => vec![other.to_string()],
        };
        let rows = libs
            .into_iter()
            .flat_map(|l| {
                host.link
                    .borrow_mut()
                    .with(|c| c.list_objects(l, name.clone(), "*JRN".into()))
            })
            .map(|o| {
                vec![
                    ("JRN".to_string(), o.name),
                    ("JRNLIB".to_string(), o.lib),
                    ("ASPDEV".to_string(), String::new()),
                    ("JRNTEXT".to_string(), o.text),
                ]
            })
            .collect();
        fill(d, "JRNS", rows);
        Ok(())
    };
    let mut keys = |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(match &resp {
            Response::Options(c)
                if c.first()
                    .is_some_and(|c| c.option == "6" || c.option == "7") =>
            {
                d.message = Some((
                    "Journals and receivers are recovered at IPL in this release.".into(),
                    false,
                ));
                Step::Stay
            }
            _ => others(d, &resp),
        })
    };
    work(job, host, surface, &mut d, "WRKJRN", &mut reload, &mut keys)
}

/// `WRKJRNA JRN(lib/name)`: the attributes; F15 the receiver directory,
/// F14 the journaled objects.
pub fn wrkjrna(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let q = qual(r, "JRN", "*LIBL");
    let mut d = dialog(host, "QGJOJRNA")?;
    let reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "RCVS", fetch(host, "JRNA", &[&q])?);
        Ok(())
    };
    reload(host, &mut d)?;
    loop {
        match show(&mut d, surface, "WRKJRNA")? {
            Response::Return(5) => reload(host, &mut d)?,
            Response::Return(15) => {
                let mut keys =
                    |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
                        Ok(match resp {
                            Response::Return(1) => Step::Leave,
                            other => no_more(d, &other),
                        })
                    };
                let mut again = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
                    load(d, "RCVS", fetch(host, "JRNA", &[&q])?);
                    Ok(())
                };
                work(
                    job, host, surface, &mut d, "WRKJRNRD", &mut again, &mut keys,
                )?;
            }
            Response::Return(14) => {
                let mut again = |_: &Host, _: &mut UimDialog| Ok(());
                display(host, surface, &mut d, "DSPJRNOBJ", &mut again)?;
            }
            _ => return Ok(String::new()),
        }
    }
}

/// `DSPJRN JRN(lib/name)`: the entries; option 5 shows one in full.
pub fn dspjrn(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let q = qual(r, "JRN", "*LIBL");
    let mut d = dialog(host, "QGJOJRNE")?;
    load(&mut d, "ENTRIES", fetch(host, "JRNE", &[&q])?);
    loop {
        let resp = show(&mut d, surface, "DSPJRN")?;
        if let Some(c) = choice(&resp) {
            let c = c.clone();
            d.list_mut("ENTRIES").options.clear();
            let mut one = dialog(host, "QGJOJRNE")?;
            for (k, v) in d.row(&c.list, c.row) {
                one.set(&k, &v);
            }
            let mut again = |_: &Host, _: &mut UimDialog| Ok(());
            display(host, surface, &mut one, "DSPJRNE", &mut again)?;
            continue;
        }
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            _ => {}
        }
    }
}

/// `DSPJRNRCVA JRNRCV(lib/name)`.
pub fn dspjrnrcva(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let q = qual(r, "JRNRCV", "*LIBL");
    let mut d = dialog(host, "QGJORCV")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "", fetch(host, "JRNRCVA", &[&q])?);
        Ok(())
    };
    display(host, surface, &mut d, "DSPJRNRCVA", &mut reload)
}
