// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPSECA` (`QUISECA`), `WRKFCNUSG` and `DSPFCNUSG` (`QUIFCNUSG`)
//! over the service's data (`lic/slic/src/cpp/fcncmd.rs`).

use cmd::Resolved;
use mi::Job;

use super::{act, dialog, fetch, parm, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

type Rows = Vec<Vec<(String, String)>>;

/// `DSPSECA`.
pub fn dspseca(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let (vars, _) = fetch(host, "SECA", &[])?;
    let mut d = dialog(host, "QUISECA")?;
    for (k, v) in &vars {
        let var = match k.as_str() {
            "UID" => "SAUID",
            "GID" => "SAGID",
            "QSECURITY" => "SASECLVL",
            "QPWDLVL" => "SAPWDLVL",
            "SYSVAL" => "SASYSVAL",
            "CERT" => "SACERT",
            "STPWD" => "SASTPWD",
            _ => continue,
        };
        d.set(var, v);
    }
    loop {
        match show(&mut d, surface, "DSPSECA")? {
            Response::Print => d.error("CPD9B84", &[]),
            _ => return Ok(String::new()),
        }
    }
}

fn get(row: &[(String, String)], k: &str) -> String {
    row.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default()
}

fn lists(data: stg::proto::Sys2Data) -> (Rows, Rows) {
    let mut fns = Vec::new();
    let mut users = Vec::new();
    for (name, rows) in data.1 {
        match name.as_str() {
            "FUNCTIONS" => fns = rows,
            "USERS" => users = rows,
            _ => {}
        }
    }
    (fns, users)
}

/// Show one function and the profiles with usage of their own, two to
/// a row; whether the operator left with F3.
fn one(host: &Host, surface: &mut Surface, f: &[(String, String)], users: &Rows) -> Result<bool, Fail> {
    let mut d = dialog(host, "QUIFCNUSG")?;
    let id = get(f, "FCNID");
    for k in ["FCNID", "FCNNAME", "FCNPROD", "FCNGRP", "FCNDFT", "FCNALLOBJ"] {
        d.set(k, &get(f, k));
    }
    // The description takes up to two lines of 50 (ours are one).
    let desc: Vec<char> = get(f, "FCNDESC").chars().collect();
    let first: String = desc.iter().take(50).collect();
    let rest: String = desc.iter().skip(50).take(50).collect();
    d.set("FCNDESC", &first);
    d.set("FCNDESC2", &rest);
    let mine: Vec<&Vec<(String, String)>> = users.iter().filter(|u| get(u, "FCNID") == id).collect();
    d.clear_list("USERS");
    for pair in mine.chunks(2) {
        let mut row = Vec::new();
        for (i, u) in pair.iter().enumerate() {
            let n = i + 1;
            row.push((format!("USER{n}"), get(u, "USER")));
            row.push((format!("TYPE{n}"), get(u, "TYPE")));
            row.push((format!("USAGE{n}"), get(u, "USAGE")));
        }
        d.add_row("USERS", row);
    }
    d.list_mut("USERS").complete = true;
    loop {
        match show(&mut d, surface, "DSPFCNUSG")? {
            Response::Print => d.error("CPD9B84", &[]),
            Response::Exit | Response::Gone => return Ok(true),
            _ => return Ok(false),
        }
    }
}

/// `DSPFCNUSG FCNID()`: each function matching, one display after
/// another.
pub fn dspfcnusg(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let pattern = match parm(r, "FCNID") {
        p if p.is_empty() => "*ALL".to_string(),
        p => p,
    };
    let (fns, users) = lists(fetch(host, "FCNUSG", &[pattern])?);
    if fns.is_empty() {
        return Err(Fail::new("CPI223E", ""));
    }
    for f in &fns {
        if one(host, surface, f, &users)? {
            break;
        }
    }
    Ok(String::new())
}

fn fill(d: &mut UimDialog, fns: &Rows) {
    let top = d.list_mut("FUNCTIONS").top;
    d.clear_list("FUNCTIONS");
    for f in fns {
        d.add_row(
            "FUNCTIONS",
            vec![
                ("OPT".into(), String::new()),
                ("FCNID".into(), get(f, "FCNID")),
                ("FCNNAME".into(), get(f, "FCNNAME")),
            ],
        );
    }
    let l = d.list_mut("FUNCTIONS");
    l.complete = true;
    l.top = if top < fns.len() { top } else { 0 };
}

/// `WRKFCNUSG FCNID()`: the functions; option 2 prompts `CHGFCNUSG`,
/// option 5 shows one.
pub fn wrkfcnusg(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let pattern = match parm(r, "FCNID") {
        p if p.is_empty() => "*ALL".to_string(),
        p => p,
    };
    let mut d = dialog(host, "QUIFCNUSG")?;
    loop {
        let (fns, users) = lists(fetch(host, "FCNUSG", std::slice::from_ref(&pattern))?);
        fill(&mut d, &fns);
        match show(&mut d, surface, "WRKFCNUSG")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                let done = act(job, host, &mut d, &choices, &mut |d, c| {
                    if c.option.trim() != "5" {
                        d.error("CPD6A7C", &[]);
                        return Ok(());
                    }
                    if let Some(f) = fns.get(c.row)
                        && one(host, surface, f, &users)?
                    {
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
            Response::Command(text) => {
                let text = d.substitute(&text, None);
                if let Some(m) = crate::panels::exec(job, host, &text) {
                    d.send(m);
                }
                d.set(crate::display::uim::CMDLINE_VAR, "");
            }
            Response::Return(4) => {
                let text = d.get(crate::display::uim::CMDLINE_VAR).trim().to_string();
                if text.is_empty() {
                    d.error("CPD9B86", &[]);
                } else {
                    if let Some(m) = crate::panels::exec(job, host, &format!("?{text}")) {
                        d.send(m);
                    }
                    d.set(crate::display::uim::CMDLINE_VAR, "");
                }
            }
            Response::Print => d.error("CPD9B84", &[]),
            _ => {}
        }
    }
}
