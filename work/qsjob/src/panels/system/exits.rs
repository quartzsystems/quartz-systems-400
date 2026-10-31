// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKREGINF` (`QUIREGINF`): the exit points, option 5 shows one
//! (`DSPEXITPNT`), option 8 works with its programs (`WRKEXITPGM`:
//! 1 add, 4 remove, 5 display `DSPEXITPGM`, 10 replace). The data comes
//! from the registration facility through the service.

use cmd::Resolved;
use mi::Job;
use regfac::{Answer, ExitPoint, ExitProgram, Query};
use stg::Client;

use super::{Next, command, display, parm, work_answer};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog};

fn ask(host: &Host, q: Query) -> Result<Answer, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.regfac(q))
        .map_err(|e| Fail::new(&e.msgid, e.detail))
}

fn points(host: &Host, name: &str, format: &str, which: u8) -> Result<Vec<ExitPoint>, Fail> {
    match ask(
        host,
        Query::List {
            name: name.into(),
            format: format.into(),
            which,
        },
    )? {
        Answer::Points { points } => Ok(points),
        _ => Ok(Vec::new()),
    }
}

fn row(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

fn yes(b: bool) -> String {
    if b { "*YES" } else { "*NO" }.to_string()
}

/// `WRKREGINF EXITPNT() FORMAT()`.
pub fn wrkreginf(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    if matches!(parm(r, "OUTPUT").as_str(), "P" | "*PRINT") {
        return None;
    }
    let sel = parm(r, "EXITPNT");
    let (name, which) = match sel.as_str() {
        "*REGISTERED" | "" => ("*ALL".to_string(), 0),
        "*UNREGISTERED" => ("*ALL".to_string(), 1),
        "*ALL" => ("*ALL".to_string(), 2),
        n => (n.to_string(), 2),
    };
    let format = match parm(r, "FORMAT") {
        f if f.is_empty() => "*ALL".to_string(),
        f => f,
    };
    let mut d = match crate::panels::dialog(host, "QUIREGINF") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    loop {
        let found = match points(host, &name, &format, which) {
            Ok(p) => p,
            Err(e) => return Some(Err(e)),
        };
        let top = d.list_mut("POINTS").top;
        d.clear_list("POINTS");
        for p in &found {
            d.add_row(
                "POINTS",
                row(&[
                    ("OPT", String::new()),
                    ("EPNAME", p.name.clone()),
                    ("EPFMT", p.format.clone()),
                    // Centered under its heading, as IBM shows it.
                    ("EPREGD", format!("   {}", yes(p.registered))),
                    ("EPTEXT", p.desc.as_text().to_string()),
                ]),
            );
        }
        let list = d.list_mut("POINTS");
        list.complete = true;
        list.top = top.min(list.rows.len().saturating_sub(1));
        loop {
            let answer = match d.show(surface, "WRKREGINF") {
                Ok(a) => a,
                Err(e) => return Some(Err(crate::panels::failed(e.to_string()))),
            };
            match answer {
                Response::Options(choices) => {
                    for choice in &choices {
                        let Some(point) = found.get(choice.row) else {
                            continue;
                        };
                        match choice.option.trim() {
                            "5" => {
                                if let Err(e) = dspexitpnt(host, surface, point) {
                                    return Some(Err(e));
                                }
                            }
                            "8" => {
                                if let Err(e) =
                                    wrkexitpgm(job, host, surface, &point.name, &point.format)
                                {
                                    return Some(Err(e));
                                }
                            }
                            o => {
                                d.message = Some((format!("Option {o} is not valid."), true));
                            }
                        }
                    }
                    d.list_mut("POINTS").options.clear();
                    break;
                }
                Response::Return(4) => prompt(job, host, &mut d),
                other => match work_answer(job, host, &mut d, "POINTS", other) {
                    Next::Stay => {}
                    Next::Refresh => break,
                    Next::Leave => return Some(Ok(String::new())),
                },
            }
        }
    }
}

/// F4: prompt the command on the command line.
fn prompt(job: &mut Job, host: &Host, d: &mut UimDialog) {
    let text = d.get(CMDLINE_VAR).trim().to_string();
    if text.is_empty() {
        d.message = Some(("Type a command, then press F4.".into(), true));
        return;
    }
    d.message = command(job, host, &format!("?{text}"));
    d.set(CMDLINE_VAR, "");
}

fn prep(p: Option<&regfac::Prep>) -> (String, String, String) {
    p.map_or_else(
        || ("*NONE".to_string(), String::new(), String::new()),
        |p| (p.pgm.clone(), p.lib.clone(), p.format.clone()),
    )
}

/// Option 5: one exit point.
fn dspexitpnt(host: &Host, surface: &mut Surface, p: &ExitPoint) -> Result<(), Fail> {
    let mut d = crate::panels::dialog(host, "QUIREGINF")?;
    d.set("EPNAME", &p.name);
    d.set("EPFMT", &p.format);
    d.set("EPREG", &yes(p.registered));
    d.set("EPDEREG", &yes(p.allow_dereg));
    d.set(
        "EPMAX",
        &p.max
            .map_or_else(|| "*NOMAX".to_string(), |m| m.to_string()),
    );
    d.set("EPCOUNT", &p.programs.len().to_string());
    for (k, v) in [
        ("EPADD", prep(p.prep_add.as_ref())),
        ("EPRMV", prep(p.prep_rmv.as_ref())),
        ("EPRTV", prep(p.prep_rtv.as_ref())),
    ] {
        d.set(&format!("{k}P"), &v.0);
        d.set(&format!("{k}L"), &v.1);
        d.set(&format!("{k}F"), &v.2);
    }
    d.set("EPCHG", &yes(p.allow_change));
    match &p.desc {
        regfac::Desc::Text { text } => {
            d.set("EPMSGF", "*NONE");
            d.set("EPTEXT", text);
        }
        regfac::Desc::Msg { msgf, lib, msgid } => {
            d.set("EPMSGF", msgf);
            d.set("EPMSGL", lib);
            d.set("EPMSGID", msgid);
        }
    }
    display(surface, &mut d, "DSPEXITPNT").map(|_| ())
}

fn programs(host: &Host, name: &str, format: &str) -> Result<Vec<ExitProgram>, Fail> {
    Ok(points(host, name, format, 2)?
        .into_iter()
        .find(|p| p.is(name, format))
        .map(|p| p.programs)
        .unwrap_or_default())
}

/// Option 8: a point's exit programs.
fn wrkexitpgm(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    name: &str,
    format: &str,
) -> Result<(), Fail> {
    let mut d = crate::panels::dialog(host, "QUIREGINF")?;
    d.set("EPNAME", name);
    d.set("EPFMT", format);
    loop {
        let found = programs(host, name, format)?;
        d.clear_list("PGMS");
        for g in &found {
            d.add_row(
                "PGMS",
                row(&[
                    ("OPT", String::new()),
                    ("PGMNBR", g.number.to_string()),
                    ("PGM", g.pgm.clone()),
                    ("PGMLIB", g.lib.clone()),
                ]),
            );
        }
        d.list_mut("PGMS").complete = true;
        loop {
            let answer = d
                .show(surface, "WRKEXITPGM")
                .map_err(|e| crate::panels::failed(e.to_string()))?;
            match answer {
                Response::Options(choices) => {
                    program_options(job, host, surface, &mut d, (name, format), &found, &choices)?;
                    break;
                }
                Response::Return(4) => prompt(job, host, &mut d),
                other => match work_answer(job, host, &mut d, "PGMS", other) {
                    Next::Stay => {}
                    Next::Refresh => break,
                    Next::Leave => return Ok(()),
                },
            }
        }
    }
}

/// The options typed on the programs: each runs its command, prompted
/// where IBM prompts (add, replace).
fn program_options(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    (name, format): (&str, &str),
    found: &[ExitProgram],
    choices: &[Choice],
) -> Result<(), Fail> {
    let mut shown = None;
    for c in choices {
        let Some(g) = found.get(c.row) else { continue };
        let point = format!("EXITPNT({name}) FORMAT({format})");
        let text = match c.option.trim() {
            "1" => format!("?ADDEXITPGM {point}"),
            "4" => format!("RMVEXITPGM {point} PGMNBR({})", g.number),
            "10" => format!(
                "?ADDEXITPGM {point} PGMNBR({}) PGM({}/{}) REPLACE(*YES)",
                g.number, g.lib, g.pgm
            ),
            "5" => {
                dspexitpgm(host, surface, (name, format), g)?;
                continue;
            }
            o => {
                shown = Some((format!("Option {o} is not valid."), true));
                continue;
            }
        };
        if let Some(m) = command(job, host, &text) {
            shown = Some(m);
        }
    }
    d.list_mut("PGMS").options.clear();
    d.message = shown;
    Ok(())
}

/// Option 5 on a program.
fn dspexitpgm(
    host: &Host,
    surface: &mut Surface,
    (name, format): (&str, &str),
    g: &ExitProgram,
) -> Result<(), Fail> {
    let mut d = crate::panels::dialog(host, "QUIREGINF")?;
    d.set("EPNAME", name);
    d.set("EPFMT", format);
    d.set("PGMNBR", &g.number.to_string());
    d.set("PGM", &g.pgm);
    d.set("PGMLIB", &g.lib);
    d.set("PGTHDS", g.threadsafe.special());
    d.set("PGMTA", g.mt_action.special());
    d.set(
        "PGCCSID",
        &if g.ccsid == 0 {
            "*JOB".to_string()
        } else {
            g.ccsid.to_string()
        },
    );
    d.set("PGDTALEN", &g.data.len().to_string());
    d.set("PGDATA", &ccsid::from_ebcdic(&g.data));
    d.set("PGTEXT", g.desc.as_text());
    display(surface, &mut d, "DSPEXITPGM").map(|_| ())
}
