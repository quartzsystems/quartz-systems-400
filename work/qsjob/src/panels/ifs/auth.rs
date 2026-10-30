// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKAUT` and `DSPAUT` (`QUIWRKAUT`): an object's authority rows from
//! `DSPAUT`'s answer. `WRKAUT` puts a blank entry first for option 1;
//! options 1 and 2 prompt `CHGAUT`, 4 takes the user's authority away.
//! F11 switches between the object authorities and the data
//! authorities one by one.

use ifs::ops::{AuthList, Role};
use ifscmd::quote;
use mi::Job;
use sec::auth::bits::NAMES;

use super::{cell, client, exec, fail, fill, row, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

const LIST: &str = "AUTS";

/// The bit of an authority name.
fn bit(name: &str) -> u32 {
    NAMES.iter().find(|(n, _)| *n == name).map_or(0, |(_, b)| *b)
}

/// An `X` in the middle of a column `width` wide, when `on`.
fn mark(on: bool, width: usize) -> String {
    if on {
        format!("{}X", " ".repeat((width - 1) / 2))
    } else {
        String::new()
    }
}

/// The dialog's rows for an answer.
fn rows(a: &AuthList, work: bool) -> Vec<Vec<(String, String)>> {
    let mut v = Vec::new();
    if work {
        v.push(Vec::new());
    }
    for e in &a.entries {
        let has = |n: &str| e.auth.0 & bit(n) != 0;
        let role = match e.role {
            Role::Public => "P",
            Role::Owner => "O",
            Role::Group => "G",
            Role::Private => "U",
        };
        v.push(row(&[
            ("USER", &e.who),
            ("DTAAUT", &e.data),
            ("EXIST", &mark(has("*OBJEXIST"), 5)),
            ("MGT", &mark(has("*OBJMGT"), 3)),
            ("ALTER", &mark(has("*OBJALTER"), 5)),
            ("REF", &mark(has("*OBJREF"), 3)),
            ("OPR", &mark(has("*OBJOPR"), 3)),
            ("READ", &mark(has("*READ"), 4)),
            ("ADD", &mark(has("*ADD"), 3)),
            ("UPD", &mark(has("*UPD"), 3)),
            ("DLT", &mark(has("*DLT"), 3)),
            ("EXE", &mark(has("*EXECUTE"), 3)),
            ("ROLE", role),
        ]));
    }
    v
}

/// The answer for `path` into the dialog.
fn load(host: &Host, d: &mut UimDialog, path: &str, work: bool) -> Result<(), Fail> {
    let c = client(host);
    let a = ifs::client::IfsClient::dspaut(&c, path).map_err(|e| fail(e, path))?;
    d.set("OBJ", &a.path);
    d.set("OBJTYPE", a.objtype.trim_start_matches('*'));
    d.set("OWNER", &a.owner);
    d.set("PGP", a.group.as_deref().unwrap_or("*NONE"));
    d.set("AUTL", a.autl.as_deref().unwrap_or("*NONE"));
    fill(d, LIST, rows(&a, work));
    Ok(())
}

/// `WRKAUT OBJ(path)` (`work`) or `DSPAUT`.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, path: &str, work: bool) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QUIWRKAUT")?;
    load(host, &mut d, path, work)?;
    let panel = if work { "WRKAUT" } else { "DSPAUT" };
    let p = quote(path);
    let mut last = String::new();
    loop {
        let mut reload = false;
        match show(&mut d, surface, panel)? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(0) if !work => return Ok(String::new()),
            Response::Return(11) => {
                d.view = 1 - d.view.min(1);
                let v = d.view.to_string();
                d.set("VIEW", &v);
            }
            Response::Return(5) => reload = true,
            Response::Return(4) => {
                let t = d.get(CMDLINE_VAR).trim().to_string();
                if t.is_empty() {
                    d.message = Some(("Type a command on the command line, then F4.".into(), true));
                } else {
                    d.set(CMDLINE_VAR, "");
                    exec(job, host, &mut d, &format!("?{}", t.trim_start_matches('?')));
                    reload = true;
                }
            }
            Response::Return(22) => d.message = Some((path.to_string(), false)),
            Response::Retrieve => {
                if last.is_empty() {
                    d.message = Some(("There is no command to retrieve.".into(), true));
                } else {
                    d.set(CMDLINE_VAR, &last);
                }
            }
            Response::Command(t) => {
                last = t.trim_start_matches('?').to_string();
                exec(job, host, &mut d, &t);
                reload = true;
            }
            Response::Options(choices) => {
                d.list_mut(LIST).options.clear();
                for ch in choices {
                    let user = cell(&d, LIST, ch.row, "USER");
                    let role = cell(&d, LIST, ch.row, "ROLE");
                    let cmd = match (ch.option.trim(), user.is_empty()) {
                        ("1", _) => format!("?CHGAUT OBJ({p})"),
                        ("2", false) => format!("?CHGAUT OBJ({p}) USER({user})"),
                        ("4", false) if role == "U" || role == "G" => {
                            format!("CHGAUT OBJ({p}) USER({user}) DTAAUT(*NONE) OBJAUT(*NONE)")
                        }
                        (o, _) => {
                            let l = d.list_mut(LIST);
                            l.options.insert(ch.row, o.to_string());
                            l.errors.insert(ch.row);
                            d.message = Some((format!("Option {o} is not allowed on this entry."), true));
                            break;
                        }
                    };
                    exec(job, host, &mut d, &cmd);
                    reload = true;
                }
            }
            _ => d.message = Some(("That function is not available here.".into(), true)),
        }
        if reload
            && let Err(f) = load(host, &mut d, path, work)
        {
            d.message = Some((f.text, true));
        }
    }
}

