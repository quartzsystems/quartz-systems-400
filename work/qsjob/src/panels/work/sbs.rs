// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKSBS` (`QUIWRKSBS`) and `DSPSBSD` (`QUIDSPSBSD`): the active
//! subsystems and what a subsystem description holds.

use cmd::Resolved;
use mi::Job;
use stg::Client;
use wm::obj::sbsd::{PoolKind, Sbsd};

use super::job::Left;
use super::{Flow, common, entry, parm, qualified, refill, render_fail};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// `WRKSBS`.
pub fn wrksbs(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QUIWRKSBS")?;
    let mut history = Vec::new();
    loop {
        fill_wrksbs(host, &mut d);
        loop {
            let flow = match d.show(surface, "WRKSBS").map_err(|e| render_fail(&e))? {
                Response::Return(11) => {
                    d.message = Some(("That view is not available in this release.".into(), true));
                    Flow::Stay
                }
                other => common(job, host, &mut d, &mut history, other),
            };
            match flow {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(String::new()),
            }
        }
    }
}

fn fill_wrksbs(host: &Host, d: &mut UimDialog) {
    let mut subsystems = host.link.borrow_mut().with(Client::sbs_list);
    subsystems.sort_by(|a, b| a.name.cmp(&b.name));
    let pools = host.link.borrow_mut().with(Client::pools);
    let rows = subsystems
        .iter()
        .map(|s| {
            let mut cols: Vec<(String, String)> = (1..=10).map(|i| (format!("P{i}"), String::new())).collect();
            let mut kb = 0;
            for (id, name) in &s.pools {
                if let Some(p) = pools.iter().find(|p| p.name == *name) {
                    if let Some(c) = cols.get_mut(usize::from(*id).saturating_sub(1)) {
                        c.1 = p.number.to_string();
                    }
                    kb += p.size_kb;
                }
            }
            let mut row = entry(&[
                ("SBS", s.name.clone()),
                ("SBSLIB", s.lib.clone()),
                ("STORAGE", super::decimal(host, kb * 10 / 1024)),
            ]);
            row.extend(cols);
            row
        })
        .collect();
    refill(d, "SBS", rows);
}

/// `DSPSBSD SBSD(lib/name) [OPTION()]`.
pub fn dspsbsd(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (lib, name) = qualified(r, "SBSD").ok_or_else(|| Fail::new("CPD0071", "SBSD"))?;
    let payload = host
        .link
        .borrow_mut()
        .with(|l| l.wm_object(format!("{lib}/{name}"), "*SBSD".into()))
        .map_err(|e| {
            let (id, text) = e.split_once(' ').unwrap_or(("CPF1619", e.as_str()));
            Fail::new(id, text)
        })?;
    let sbsd = Sbsd::decode(&payload).map_err(|e| crate::panels::failed(e.to_string()))?;
    let real_lib = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), "*SBSD".into()))
        .map_or(lib, |o| o.lib);
    let active = host
        .link
        .borrow_mut()
        .with(Client::sbs_list)
        .iter()
        .any(|s| s.name == name);
    let mut d = crate::panels::dialog(host, "QUIDSPSBSD")?;
    d.set("SBS", &name);
    d.set("SBSLIB", &real_lib);
    d.set("STATUS", if active { "ACTIVE" } else { "INACTIVE" });
    fill_sbsd(&mut d, &sbsd);
    let direct = match parm(r, "OPTION").as_deref() {
        None | Some("*ALL" | "*SELECT") => None,
        Some("*OPRATR") => Some(1),
        Some("*POOLS") => Some(2),
        Some("*AJE") => Some(3),
        Some("*WSNE" | "*WSNAME") => Some(4),
        Some("*WSTE" | "*WSTYPE") => Some(5),
        Some("*JOBQE" | "*JOBQ") => Some(6),
        Some("*RTGE") => Some(7),
        Some("*CMNE") => Some(8),
        Some("*RMTLE" | "*RMTLOCNAME") => Some(9),
        Some("*PJE") => Some(10),
        Some(o) => return Err(Fail::new("CPD0084", format!("OPTION({o}) is not valid"))),
    };
    if let Some(n) = direct {
        entries(job, host, surface, &mut d, n, &name)?;
        return Ok(String::new());
    }
    let mut history = Vec::new();
    loop {
        match d.show(surface, "DSPSBSD").map_err(|e| render_fail(&e))? {
            Response::Return(n) if n > 100 => {
                if let Left::Exit = entries(job, host, surface, &mut d, n - 100, &name)? {
                    return Ok(String::new());
                }
            }
            Response::Return(0) => {}
            other => {
                if let Flow::Leave = common(job, host, &mut d, &mut history, other) {
                    return Ok(String::new());
                }
            }
        }
    }
}

/// `*NOMAX` for 0.
fn nomax(n: u32) -> String {
    if n == 0 { "*NOMAX".into() } else { n.to_string() }
}

/// `LIB/NAME` parts shown as name and library.
fn pair(p: &(String, String)) -> (String, String) {
    (p.0.clone(), p.1.clone())
}

fn pool_text(kind: PoolKind, size_kb: u64) -> String {
    match kind {
        PoolKind::Private => (size_kb / 1024).to_string(),
        PoolKind::Base => "*BASE".into(),
        PoolKind::Interact => "*INTERACT".into(),
        PoolKind::Spool => "*SPOOL".into(),
        PoolKind::Shared(n) => format!("*SHRPOOL{n}"),
        PoolKind::Machine => "*MACHINE".into(),
    }
}

fn fill_sbsd(d: &mut UimDialog, s: &Sbsd) {
    d.set("MAXJOBS", &nomax(s.maxjobs));
    let (dspf, dspf_lib) = pair(&s.sgndspf);
    d.set("SGNDSPF", &dspf);
    d.set("SGNLIB", &dspf_lib);
    d.set("SYSLIBLE", if s.syslible.is_empty() { "*NONE" } else { &s.syslible });
    d.set("ASPGRP", "*NONE");
    d.set("WLGRP", "*NONE");
    let pools = s
        .pools
        .iter()
        .map(|p| {
            let private = p.kind == PoolKind::Private;
            entry(&[
                ("POOLID", p.id.to_string()),
                ("POOLSIZE", pool_text(p.kind, p.size_kb)),
                ("ACTLVL", if private { p.actlvl.to_string() } else { String::new() }),
            ])
        })
        .collect();
    refill(d, "POOLS", pools);
    let ajes = s
        .ajes
        .iter()
        .map(|a| {
            entry(&[
                ("AJEJOB", a.job.clone()),
                ("AJEJOBD", a.jobd.0.clone()),
                ("AJELIB", a.jobd.1.clone()),
            ])
        })
        .collect();
    refill(d, "AJES", ajes);
    let jobqes = s
        .jobqes
        .iter()
        .map(|q| {
            let mut row = entry(&[
                ("SEQ", q.seq.to_string()),
                ("JOBQ", q.jobq.0.clone()),
                ("JOBQLIB", q.jobq.1.clone()),
                ("MAXACT", nomax(u32::from(q.maxact))),
            ]);
            for (i, m) in q.maxpty.iter().enumerate() {
                let v = if *m == 0 { "*".to_string() } else { m.to_string() };
                row.push((format!("M{}", i + 1), v));
            }
            row
        })
        .collect();
    refill(d, "JOBQES", jobqes);
    let rtges = s
        .rtges
        .iter()
        .map(|r| {
            let any = r.cmpval == "*ANY";
            entry(&[
                ("RTSEQ", r.seq.to_string()),
                ("PGM", r.pgm.0.clone()),
                ("PGMLIB", r.pgm.1.clone()),
                ("CMPVAL", if any { r.cmpval.clone() } else { format!("'{}'", r.cmpval) }),
                ("STARTPOS", if any { String::new() } else { r.start.to_string() }),
            ])
        })
        .collect();
    refill(d, "RTGES", rtges);
    let cmnes = s
        .cmnes
        .iter()
        .map(|c| {
            entry(&[
                ("DEV", c.dev.clone()),
                ("MODE", c.mode.clone()),
                ("CMNJOBD", c.jobd.clone()),
                ("MAXACT", nomax(u32::from(c.maxact))),
            ])
        })
        .collect();
    refill(d, "CMNES", cmnes);
    let pjes = s
        .pjes
        .iter()
        .map(|p| {
            entry(&[
                ("PJEPGM", p.pgm.0.clone()),
                ("PJELIB", p.pgm.1.clone()),
                ("PJEUSER", p.user.clone()),
                ("PJEJOB", p.job.clone()),
            ])
        })
        .collect();
    refill(d, "PJES", pjes);
    // The work station lists are filled when their panel shows.
    let names = ws_rows(&s.wsns);
    refill(d, "WSNAMES", names.clone());
    let types = ws_rows(&s.wsts);
    refill(d, "WSTYPES", types);
}

fn ws_rows(list: &[wm::obj::sbsd::Wse]) -> Vec<Vec<(String, String)>> {
    list.iter()
        .map(|w| {
            entry(&[
                ("WSNAME", w.name.clone()),
                ("WSJOBD", w.jobd.0.clone()),
                ("WSJOBDL", w.jobd.1.clone()),
                ("WSMAX", nomax(u32::from(w.maxact))),
            ])
        })
        .collect()
}

/// One option of the menu: its display, or each in turn for 20.
fn entries(job: &mut Job, host: &Host, surface: &mut Surface, d: &mut UimDialog, n: u32, name: &str) -> Result<Left, Fail> {
    if n == 21 {
        super::run_command(job, host, d, &mut Vec::new(), &format!("WRKSBSJOB SBS({name})"));
        return Ok(Left::Back);
    }
    if n == 20 {
        for k in [1, 2, 3, 4, 5, 6, 7, 8, 10] {
            if let Left::Exit = entries(job, host, surface, d, k, name)? {
                return Ok(Left::Exit);
            }
        }
        return Ok(Left::Back);
    }
    let panel = match n {
        1 => "OPRATR",
        2 => "POOLS",
        3 => "AJES",
        4 | 5 => {
            // One list serves both work station panels.
            let from = if n == 4 { "WSNAMES" } else { "WSTYPES" };
            let rows = d.list_mut(from).rows.clone();
            refill(d, "WSES", rows);
            if n == 4 { "WSNES" } else { "WSTES" }
        }
        6 => "JOBQES",
        7 => "RTGES",
        8 => "CMNES",
        10 => "PJES",
        _ => {
            d.message = Some((format!("Option {n} is not available in this release."), true));
            return Ok(Left::Back);
        }
    };
    d.page = 0;
    let mut history = Vec::new();
    loop {
        match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Exit | Response::Gone => return Ok(Left::Exit),
            Response::Cancel | Response::Return(1) => return Ok(Left::Back),
            Response::Options(_) | Response::Return(9) => {
                d.list_mut("WSES").options.clear();
                d.list_mut("RTGES").options.clear();
                d.message = Some(("The detailed descriptions are not available in this release.".into(), true));
            }
            other => {
                if let Flow::Leave = common(job, host, d, &mut history, other) {
                    return Ok(Left::Back);
                }
            }
        }
    }
}
