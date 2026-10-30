// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKLNK` and `DSPLNK`: the links a path names. Option 5 on a
//! directory shows its contents in place (F12 comes back up), on a
//! stream file it browses it; F23 steps through the option legends
//! (`WRKLNK1` to `WRKLNK3`; every option works on each).

use cmd::Resolved;
use ifs::client::IfsClient;
use ifscmd::display::{self, Link};
use ifscmd::quote;
use mi::Job;

use super::{cell, client, exec, fail, fill, row, show, text};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

const LIST: &str = "LNKS";

/// The first screen row of the list's entries (0-based).
const FIRST_ROW: usize = 9;

/// One level the display has shown: the pattern and the type filter.
#[derive(Clone)]
struct Level {
    obj: String,
    objtype: String,
}

/// `WRKLNK` (`work`) or `DSPLNK`.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, work: bool) -> Result<String, Fail> {
    let c = client(host);
    let obj = match text(r, "OBJ") {
        s if s.is_empty() => "*".to_string(),
        s => s,
    };
    let objtype = match text(r, "OBJTYPE") {
        s if s.is_empty() => "*ALL".to_string(),
        s => s,
    };
    let mut d = crate::panels::dialog(host, "QUIWRKLNK")?;
    let mut stack = vec![Level { obj, objtype }];
    // The first level must exist: its error ends the command.
    load(&c, &mut d, &stack[0]).map_err(|e| fail(e, &stack[0].obj))?;
    let mut set = 0usize;
    let mut last = String::new();
    loop {
        let panel = if work {
            ["WRKLNK1", "WRKLNK2", "WRKLNK3"][set]
        } else {
            "DSPLNK"
        };
        let resp = show(&mut d, surface, panel)?;
        let mut reload = false;
        match resp {
            Response::Exit | Response::Gone => return Ok(String::new()),
            Response::Cancel => {
                stack.pop();
                if stack.is_empty() {
                    return Ok(String::new());
                }
                d.list_mut(LIST).top = 0;
                reload = true;
            }
            Response::Return(23) => set = (set + 1) % 3,
            Response::Return(5) => reload = true,
            Response::Return(4) => {
                let t = d.get(CMDLINE_VAR).trim().to_string();
                if t.is_empty() {
                    d.message = Some(("Type a command on the command line, then F4.".into(), true));
                } else {
                    d.set(CMDLINE_VAR, "");
                    last.clone_from(&t);
                    exec(job, host, &mut d, &format!("?{}", t.trim_start_matches('?')));
                    reload = true;
                }
            }
            Response::Return(17) => position(&mut d),
            Response::Return(22) => whole_name(&mut d),
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
                    let link = link_at(&d, ch.row);
                    match option(job, host, surface, &c, &mut d, &ch.option, &link, work)? {
                        Next::Done => reload = true,
                        Next::Down(path) => {
                            stack.push(Level {
                                obj: path,
                                objtype: "*ALL".into(),
                            });
                            d.list_mut(LIST).top = 0;
                            reload = true;
                            break;
                        }
                        Next::Refused => {
                            let l = d.list_mut(LIST);
                            l.options.insert(ch.row, ch.option.clone());
                            l.errors.insert(ch.row);
                            d.message = Some((
                                format!("Option {} is not allowed on {}.", ch.option, link.name),
                                true,
                            ));
                            break;
                        }
                    }
                }
            }
            _ => {
                d.message = Some(("That function is not available here.".into(), true));
            }
        }
        if reload {
            let level = stack.last().expect("a level").clone();
            if let Err(e) = load(&c, &mut d, &level) {
                d.message = Some((format!("{}: {}", level.obj, e.text()), true));
                fill(&mut d, LIST, Vec::new());
            }
        }
    }
}

/// What an option leaves the display to do.
enum Next {
    /// Show the level again.
    Done,
    /// Show this path's contents as a new level.
    Down(String),
    /// The option does not apply to the entry.
    Refused,
}

/// The listing of a level into the dialog.
fn load(c: &dyn IfsClient, d: &mut UimDialog, level: &Level) -> ifs::Result<()> {
    let l = display::links(c, &level.obj, &level.objtype)?;
    d.set("DIR", &l.dir);
    let rows = l
        .links
        .iter()
        .map(|k| {
            row(&[
                ("LNK", &display::shown_name(&k.name)),
                ("LNKTYPE", &k.objtype),
                ("LNKATR", &k.attr),
                ("LNKTEXT", &k.text),
                ("PATH", &k.path),
                ("NAME", &k.name),
            ])
        })
        .collect();
    fill(d, LIST, rows);
    Ok(())
}

/// The link of an entry.
fn link_at(d: &UimDialog, i: usize) -> Link {
    Link {
        name: cell(d, LIST, i, "NAME"),
        path: cell(d, LIST, i, "PATH"),
        objtype: cell(d, LIST, i, "LNKTYPE"),
        attr: cell(d, LIST, i, "LNKATR"),
        text: cell(d, LIST, i, "LNKTEXT"),
    }
}

/// One option on one entry.
#[allow(clippy::too_many_arguments)] // the job, its display and the row
fn option(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    c: &dyn IfsClient,
    d: &mut UimDialog,
    opt: &str,
    link: &Link,
    work: bool,
) -> Result<Next, Fail> {
    let p = quote(&link.path);
    let dir = link.is_dir();
    let cmd = match (opt.trim(), work) {
        ("5", _) if dir => return Ok(Next::Down(display::join(&link.path, "*"))),
        ("5", _) => format!("DSPF STMF({p})"),
        ("8", _) => {
            super::attrs::show(host, surface, c, &link.path)?;
            return Ok(Next::Done);
        }
        ("9", _) => {
            super::auth::run(job, host, surface, &link.path, work)?;
            return Ok(Next::Done);
        }
        ("2", true) => format!("EDTF STMF({p})"),
        ("3", true) => format!("?CPY OBJ({p})"),
        ("4", true) if dir => format!("RD DIR({p})"),
        ("4", true) => format!("RMVLNK OBJLNK({p})"),
        ("7", true) => format!("?RNM OBJ({p})"),
        ("10", true) => format!("?MOV OBJ({p})"),
        ("11", true) if dir => format!("CD DIR({p})"),
        ("12", true) if dir => return Ok(Next::Refused),
        ("12", true) => return Ok(Next::Down(link.path.clone())),
        ("13" | "14", true) => format!("?CHGATR OBJ({p})"),
        _ => return Ok(Next::Refused),
    };
    exec(job, host, d, &cmd);
    Ok(Next::Done)
}

/// F17: the first entry at or after the command line's name.
fn position(d: &mut UimDialog) {
    let want = d.get(CMDLINE_VAR).trim().to_uppercase();
    d.set(CMDLINE_VAR, "");
    if want.is_empty() {
        d.message = Some(("Type a name on the command line, then F17.".into(), true));
        return;
    }
    let names: Vec<String> = (0..d.list_mut(LIST).rows.len())
        .map(|i| cell(d, LIST, i, "NAME").to_uppercase())
        .collect();
    match names.iter().position(|n| *n >= want) {
        Some(i) => d.list_mut(LIST).top = i,
        None => d.message = Some((format!("No entry is at or after {want}."), true)),
    }
}

/// F22: the whole name of the entry under the cursor.
fn whole_name(d: &mut UimDialog) {
    let (r, _) = d.cursor;
    let top = d.list_mut(LIST).top;
    let n = d.list_mut(LIST).rows.len();
    let i = top + r.saturating_sub(FIRST_ROW);
    if r >= FIRST_ROW && i < n {
        let path = cell(d, LIST, i, "PATH");
        d.message = Some((path, false));
    } else {
        let dir = d.get("DIR");
        d.message = Some((format!("Directory: {dir}"), false));
    }
}
