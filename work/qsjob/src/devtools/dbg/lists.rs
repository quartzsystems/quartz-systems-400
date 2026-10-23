// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays Display Module Source opens: Work with Module
//! Breakpoints, Work with Module List, Select View, Work with Watch,
//! Work with Debugged Threads, Evaluate Expression, help, the program's
//! output, a command line; and `DSPBKP`'s Display Breakpoints and the
//! Display Breakpoint stop. Each returns `None` when the display is gone.

use dbg::Effect;
use dbg::screen::{eval, help as helpscr, lists as scr, opm};
use mi::Job;
use mi::debug::{Resume, Stop};
use uim::Screen;
use uim::render::Attr;

use super::ui::{self, Key};
use super::{State, sysname};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// Show a screen until Enter, F3 or F12; `None` when gone.
fn until_left(surface: &mut Surface, screen: &Screen) -> Option<()> {
    loop {
        let a = ui::show(surface, screen)?;
        if matches!(a.key, Key::Enter | Key::F(3 | 12)) {
            return Some(());
        }
    }
}

/// A command typed on a list's command line, run; its first line.
fn run_cmdline(job: &mut Job, host: &Host, text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let o = crate::command::run(job, host, text);
    o.text.lines().next().map(str::to_string)
}

/// Work with Module Breakpoints for the module shown.
pub fn wrkmodbkp(job: &mut Job, host: &Host, surface: &mut Surface, st: &mut State) -> Option<()> {
    let sys = sysname(host);
    let mut first = 0;
    let mut msg: Option<String> = None;
    loop {
        let m = st.session.module()?.clone();
        let (lib, name) = m.lib_name();
        let rows: Vec<scr::BkptRow> = st
            .session
            .bkpts
            .of_module(&m.program, &m.module)
            .into_iter()
            .map(|b| scr::BkptRow {
                line: b.line,
                condition: b.when.clone().unwrap_or_default(),
                thread: b.thread,
            })
            .collect();
        let h = scr::Header {
            sysname: &sys,
            program: name,
            library: lib,
            module: &m.module,
            kind: &m.kind,
        };
        let screen = scr::wrkmodbkp(&h, &rows, first, "", msg.as_deref());
        let a = ui::show(surface, &screen)?;
        msg = None;
        match a.key {
            Key::F(3 | 12) => return Some(()),
            Key::PageDown if first + scr::BKPT_ROOM < rows.len() => first += scr::BKPT_ROOM,
            Key::PageUp => first = first.saturating_sub(scr::BKPT_ROOM),
            Key::Enter => {
                for (i, o) in &a.options {
                    match (o.as_str(), *i) {
                        ("1", usize::MAX) => {
                            let line = a.up("LINE").parse::<usize>().unwrap_or(0);
                            let cond = a.vars.get("COND").map(|c| c.trim().to_string());
                            let cond = cond.filter(|c| !c.is_empty());
                            msg = Some(match st.session.add_break(line, cond, None) {
                                Ok(l) => format!("Breakpoint set on line {l}."),
                                Err(e) => e.to_string(),
                            });
                        }
                        ("4", i) => {
                            if let Some(r) = rows.get(i) {
                                st.session.bkpts.clear(&m.program, &m.module, r.line);
                                msg = Some(format!("Breakpoint removed from line {}.", r.line));
                            }
                        }
                        (other, _) => msg = Some(format!("Option {other} is not valid here.")),
                    }
                }
                if a.options.is_empty() {
                    msg = run_cmdline(job, host, &a.cmdline);
                }
                super::sync(job, host, st);
            }
            _ => {}
        }
    }
}

/// Work with Module List; the message for the source display when
/// left (empty for none).
pub fn wrkmodlst(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    st: &mut State,
) -> Option<String> {
    let sys = sysname(host);
    let mut msg: Option<String> = None;
    loop {
        // Programs with their modules under them.
        let mut rows: Vec<(scr::ModRow, Option<usize>)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (i, m) in st.session.modules.iter().enumerate() {
            if !seen.contains(&m.program) {
                seen.push(m.program.clone());
                let (lib, name) = m.lib_name();
                rows.push((
                    scr::ModRow {
                        name: name.to_string(),
                        library: lib.to_string(),
                        kind: m.kind.clone(),
                        selected: false,
                    },
                    None,
                ));
            }
            rows.push((
                scr::ModRow {
                    name: m.module.clone(),
                    library: String::new(),
                    kind: "*MODULE".into(),
                    selected: i == st.session.shown,
                },
                Some(i),
            ));
        }
        let shown: Vec<scr::ModRow> = rows.iter().map(|(r, _)| r.clone()).collect();
        let a = ui::show(surface, &scr::wrkmodlst(&sys, &shown, 0, "", msg.as_deref()))?;
        msg = None;
        match a.key {
            Key::F(3 | 12) => return Some(String::new()),
            Key::Enter => {
                if a.options.is_empty() {
                    msg = run_cmdline(job, host, &a.cmdline);
                    continue;
                }
                for (i, o) in &a.options {
                    let row = rows.get(*i);
                    match (o.as_str(), row) {
                        ("1", None) => {
                            let name = a.up("PGM");
                            let lib = Some(a.up("LIB")).filter(|l| !l.is_empty()).unwrap_or_else(|| "*LIBL".into());
                            msg = Some(add(job, host, st, &lib, &name));
                        }
                        ("4", Some((r, None))) => {
                            let program = format!("{}/{}", r.library, r.name);
                            st.session.modules.retain(|m| m.program != program);
                            st.session.bkpts.clear_program(&program);
                            if let Some(d) = job.debug.as_mut() {
                                d.programs.retain(|p| *p != program);
                            }
                            st.session.show(0);
                            if st.session.modules.is_empty() {
                                st.session.view = None;
                            }
                            super::sync(job, host, st);
                            msg = Some(format!("{program} removed from debug."));
                        }
                        ("5", Some((_, Some(m)))) => {
                            st.session.show(*m);
                            return Some(String::new());
                        }
                        ("5", Some((_, None))) => {
                            if let Some(m) = rows.get(i + 1).and_then(|(_, m)| *m) {
                                st.session.show(m);
                            }
                            return Some(String::new());
                        }
                        ("8", Some((_, Some(m)))) => {
                            st.session.show(*m);
                            wrkmodbkp(job, host, surface, st)?;
                        }
                        (other, _) => msg = Some(format!("Option {other} is not valid on that entry.")),
                    }
                }
            }
            _ => {}
        }
    }
}

/// Option 1 on the module list: one more program under debug.
fn add(job: &mut Job, host: &Host, st: &mut State, lib: &str, name: &str) -> String {
    if name.is_empty() {
        return "Type the program's name.".into();
    }
    if st.serving.is_some() {
        if let Err(e) = super::remote::order(
            host,
            stg::proto::DbgOrder::Start {
                program: format!("{lib}/{name}"),
                updprod: st.session.updprod,
            },
        ) {
            return e.text;
        }
    } else if let Err(e) = job.strdbg(name, lib) {
        return e;
    }
    match super::load(host, lib, name) {
        Ok(m) => {
            super::add_program(st, m);
            format!("{lib}/{name} added to debug.")
        }
        Err(e) => e,
    }
}

/// Select View: the module's views (one each compiled).
pub fn select_view(surface: &mut Surface, st: &mut State) -> Option<()> {
    let view = st
        .session
        .view
        .as_ref()
        .map_or("*STATEMENT", |v| v.view.as_str())
        .to_string();
    let names = eval::view_names(&view, "");
    let current = names.first().cloned().unwrap_or_default();
    let mut msg = None;
    loop {
        let a = ui::show(surface, &eval::select_view(&current, &names, msg))?;
        match a.key {
            Key::F(12 | 3) => return Some(()),
            Key::Enter if a.options.is_empty() => return Some(()),
            Key::Enter => {
                if a.options.values().any(|o| o != "1") {
                    msg = Some("Type 1 to select a view.");
                    continue;
                }
                return Some(());
            }
            _ => {}
        }
    }
}

/// Work with Watch.
pub fn wrkwatch(job: &mut Job, host: &Host, surface: &mut Surface, st: &mut State) -> Option<()> {
    let sys = sysname(host);
    let mut msg: Option<String> = None;
    loop {
        let rows: Vec<scr::WatchRow> = st
            .session
            .watches
            .list
            .iter()
            .map(|w| scr::WatchRow {
                n: w.n,
                variable: w.expr.clone(),
                address: format!("{}/{}", w.program, w.procedure),
                len: w.len,
            })
            .collect();
        let a = ui::show(surface, &scr::wrkwatch(&sys, &rows, 0, "", msg.as_deref()))?;
        msg = None;
        match a.key {
            Key::F(3 | 12) => return Some(()),
            Key::Enter => {
                if a.options.is_empty() {
                    msg = run_cmdline(job, host, &a.cmdline);
                }
                for (i, o) in &a.options {
                    let Some(w) = rows.get(*i) else { continue };
                    match o.as_str() {
                        "4" => {
                            st.session.watches.remove(w.n);
                            msg = Some(format!("Watch {} removed.", w.n));
                        }
                        "5" => {
                            msg = Some(match job.eval(&w.variable) {
                                Ok(v) => format!("{} = {v}", w.variable),
                                Err(e) => e,
                            });
                        }
                        other => msg = Some(format!("Option {other} is not valid here.")),
                    }
                }
            }
            _ => {}
        }
    }
}

/// Work With Debugged Threads: a job runs one thread.
pub fn threads(job: &mut Job, host: &Host, surface: &mut Surface, stopped: bool) -> Option<()> {
    let rows = [scr::ThreadRow {
        id: "00000001".into(),
        status: "*ENABLED".into(),
    }];
    let stop = if stopped { "00000001" } else { "*NONE" };
    let mut msg: Option<String> = None;
    loop {
        let a = ui::show(surface, &scr::threads("00000001", stop, &rows, "", msg.as_deref()))?;
        msg = None;
        match a.key {
            Key::Enter if !a.options.is_empty() => {
                msg = Some("A job here runs one thread; it is always the current one.".into());
            }
            Key::Enter if !a.cmdline.is_empty() => msg = run_cmdline(job, host, &a.cmdline),
            Key::F(3 | 12) | Key::Enter => return Some(()),
            _ => {}
        }
    }
}

/// Evaluate Expression with the history so far.
pub fn evaluate(surface: &mut Surface, st: &mut State) -> Option<()> {
    let mut first = eval::last_page(st.evals.len());
    loop {
        let a = ui::show(surface, &eval::evaluate(&st.evals, first, "", None))?;
        match a.key {
            Key::PageDown => first = (first + eval::PAGE).min(st.evals.len().saturating_sub(1)),
            Key::PageUp => first = first.saturating_sub(eval::PAGE),
            Key::Enter if !a.cmdline.is_empty() => {
                let answer = match st.session.line(&a.cmdline, None) {
                    Ok(Effect::Message(m)) => m,
                    Ok(_) => String::new(),
                    Err(e) => e.to_string(),
                };
                st.evals.extend(eval::history_entry(&a.cmdline, &answer));
                first = eval::last_page(st.evals.len());
            }
            _ => return Some(()),
        }
    }
}

/// The debugger's help: F1's (`extended`) or the `HELP` command's.
pub fn help(surface: &mut Surface, extended: bool) -> Option<()> {
    let lines = helpscr::lines();
    let pages = lines.len().div_ceil(helpscr::PAGE).max(1);
    let mut page = 0;
    loop {
        let a = ui::show(surface, &helpscr::help("Display Module Source - Help", &lines, page, extended))?;
        match a.key {
            Key::PageDown if page + 1 < pages => page += 1,
            Key::PageUp => page = page.saturating_sub(1),
            Key::F(10) => page = 0,
            Key::PageDown => {}
            _ => return Some(()),
        }
    }
}

/// F23: the program's output so far (what it wrote to the job's
/// console).
pub fn output(surface: &mut Surface, st: &State) -> Option<()> {
    let mut s = dbg::screen::blank();
    dbg::screen::title(&mut s, "Program Output");
    if st.output.is_empty() {
        dbg::screen::put(&mut s, 3, 2, Attr::Normal, "(The program has written nothing.)");
    }
    for (i, l) in st.output.iter().rev().take(18).rev().enumerate() {
        dbg::screen::put(&mut s, 3 + i, 2, Attr::Normal, l);
    }
    dbg::screen::keys(&mut s, &[(23, 2, "F12=Cancel")]);
    until_left(surface, &s)
}

/// F21: the command window over the display `under`.
pub fn command_entry(job: &mut Job, host: &Host, surface: &mut Surface, under: &Screen) -> Option<()> {
    let mut msg: Option<String> = None;
    let mut last = String::new();
    let mut cmd = String::new();
    loop {
        let a = ui::show(surface, &scr::command_window(under, &cmd, msg.as_deref()))?;
        cmd.clear();
        match a.key {
            Key::Enter if !a.cmdline.is_empty() => {
                last.clone_from(&a.cmdline);
                msg = run_cmdline(job, host, &a.cmdline);
            }
            Key::F(9) => cmd.clone_from(&last),
            Key::F(4) if !a.cmdline.is_empty() => {
                msg = run_cmdline(job, host, &format!("?{}", a.cmdline));
            }
            Key::Enter | Key::F(4) => {}
            _ => return Some(()),
        }
    }
}

/// `DSPBKP`: the statement breakpoints with their programs.
pub fn dspbkp(job: &Job, host: &Host, st: &State) {
    let mut rows: Vec<(String, String, u32)> = Vec::new();
    let mut all: Vec<(String, u32)> = st.stmts.clone();
    if let Some(d) = &job.debug {
        all.extend(d.breakpoints.iter().cloned());
    }
    all.sort();
    all.dedup();
    for (p, s) in all {
        let (lib, name) = p.split_once('/').unwrap_or(("", p.as_str()));
        rows.push((name.to_string(), lib.to_string(), s));
    }
    let mut surface = Surface::new(host.clone());
    let mut first = 0;
    loop {
        let Some(a) = ui::show(&mut surface, &opm::dspbkp(&rows, first, None)) else {
            return;
        };
        match a.key {
            Key::PageDown if first + 14 < rows.len() => first += 14,
            Key::PageUp => first = first.saturating_sub(14),
            Key::PageDown => {}
            _ => return,
        }
    }
}

/// A stop at an `ADDBKP` breakpoint: Display Breakpoint.
pub fn display_breakpoint(job: &mut Job, surface: &mut Surface, at: &Stop) -> Resume {
    let (lib, name) = at.program.split_once('/').unwrap_or(("", at.program.as_str()));
    loop {
        let s = opm::display_breakpoint(at.stmt, name, lib, &at.procedure, at.depth, None);
        let Some(a) = ui::show(surface, &s) else {
            return Resume::Go;
        };
        match a.key {
            Key::F(10) => {
                let host = surface.host().clone();
                if command_entry(job, &host, surface, &s).is_none() {
                    return Resume::Go;
                }
            }
            _ => return Resume::Go,
        }
    }
}
