// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display Module Source: the view, the debug command line and every
//! function key. A servicing job also watches for the serviced job's
//! stops while the display waits.

use dbg::Effect;
use dbg::command::{Amount, Command, Format};
use dbg::screen::source::{SourceScreen, dspmodsrc, line_at, word_at};
use mi::Job;
use mi::debug::Resume;

use super::ui::{self, Key};
use super::{State, lists, remote};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// How the display was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Back to where the command came from (F3 or F12 when not stopped).
    Leave,
    /// Go on: run, step over or step into.
    Resume(Resume),
    /// The display is gone.
    Gone,
}

/// The display until it is left.
#[allow(clippy::too_many_lines)] // one arm per key
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, st: &mut State) -> Exit {
    let mut message: Option<String> = st
        .session
        .stopped
        .as_ref()
        .map(|s| stop_text(st, s.stmt, s.reason));
    let mut cmdline = String::new();
    loop {
        let screen = {
            let m = st.session.module();
            let (lib, name) = m.map_or(("", ""), dbg::session::Module::lib_name);
            let module = m.map_or("", |m| m.module.as_str());
            let bkpts = m.map_or_else(Vec::new, |m| st.session.bkpts.lines(&m.program, &m.module));
            dspmodsrc(&SourceScreen {
                program: name,
                library: lib,
                module,
                view: st.session.view.as_ref(),
                breakpoints: &bkpts,
                keys: st.keys,
                cmdline: &cmdline,
                message: message.as_deref(),
            })
        };
        cmdline.clear();
        let serving = st.serving.is_some() && st.session.stopped.is_none();
        let mut poll = || serving && remote::has_events(host);
        let Some(got) = ui::show_or(surface, &screen, &mut poll) else {
            return Exit::Gone;
        };
        let ui::Shown::Answer(a) = got else {
            // The serviced job stopped.
            if let Some(m) = remote::take_stop(job, host, st) {
                message = Some(m);
            }
            continue;
        };
        message = None;
        let stopped = st.session.stopped.is_some();
        let cursor_line = st
            .session
            .view
            .as_ref()
            .and_then(|v| line_at(v, a.cursor.0));
        let cursor_word = st
            .session
            .view
            .as_ref()
            .and_then(|v| word_at(v, a.cursor.0, a.cursor.1));
        match a.key {
            Key::Enter => {
                if a.cmdline.is_empty() {
                    continue;
                }
                st.last.clone_from(&a.cmdline);
                match st.session.line(&a.cmdline, cursor_line) {
                    Ok(effect) => {
                        let shows = matches!(effect, Effect::Eval { .. });
                        let exit =
                            effect_of(job, host, surface, st, effect, &a.cmdline, &mut message);
                        if !shows {
                            let answer = message.clone().unwrap_or_default();
                            st.evals
                                .extend(dbg::screen::eval::history_entry(&a.cmdline, &answer));
                        }
                        if let Some(exit) = exit {
                            return exit;
                        }
                    }
                    Err(e) => {
                        message = Some(e.to_string());
                        st.evals
                            .extend(dbg::screen::eval::history_entry(&a.cmdline, &e.to_string()));
                        cmdline = failed(&a.cmdline);
                    }
                }
                super::sync(job, host, st);
            }
            Key::F(3) => {
                if stopped {
                    // F3 ends the program's debugging run: the
                    // breakpoints stay, the program runs to its end.
                    st.steps_left = 0;
                    return Exit::Resume(Resume::Go);
                }
                return Exit::Leave;
            }
            Key::F(5) => {
                if let Some(m) = st.session.module().cloned() {
                    let (lib, name) = m.lib_name();
                    if let Ok(mods) = super::load(host, lib, name) {
                        for n in mods {
                            if let Some(o) = st
                                .session
                                .modules
                                .iter_mut()
                                .find(|o| o.program == n.program && o.module == n.module)
                            {
                                *o = n;
                            }
                        }
                    }
                    let top = st.session.view.as_ref().map_or(1, |v| v.top);
                    st.session.show(st.session.shown);
                    if let Some(v) = st.session.view.as_mut() {
                        v.top = top.min(v.len().max(1));
                    }
                }
            }
            Key::F(6) => match cursor_line {
                Some(l) => {
                    message = Some(st.session.toggle(l).unwrap_or_else(|e| e.to_string()));
                    super::sync(job, host, st);
                }
                None => message = Some(no_source_or("Put the cursor on a source line first.", st)),
            },
            Key::F(9) => cmdline.clone_from(&st.last),
            Key::F(10 | 22) => {
                let into = a.key == Key::F(22);
                st.steps_left = 1;
                return Exit::Resume(if into {
                    Resume::StepInto
                } else {
                    Resume::StepOver
                });
            }
            Key::F(11) => match (stopped, cursor_word) {
                (false, _) => message = Some("The program is not stopped; no variable to show.".into()),
                (true, None) => message = Some("Put the cursor on a variable first.".into()),
                (true, Some(w)) => {
                    let effect = Effect::Eval {
                        expr: w.clone(),
                        assign: None,
                        format: Format::Natural,
                        len: None,
                    };
                    if let Some(exit) = effect_of(
                        job,
                        host,
                        surface,
                        st,
                        effect,
                        &format!("EVAL {w}"),
                        &mut message,
                    ) {
                        return exit;
                    }
                }
            },
            Key::F(12) => {
                if stopped {
                    return Exit::Resume(Resume::Go);
                }
                return Exit::Leave;
            }
            Key::F(13) => {
                if st.session.view.as_ref().is_none_or(dbg::SourceView::is_empty) {
                    message = Some(no_source_or("", st));
                } else if lists::wrkmodbkp(job, host, surface, st).is_none() {
                    return Exit::Gone;
                }
            }
            Key::F(14) => match lists::wrkmodlst(job, host, surface, st) {
                None => return Exit::Gone,
                Some(m) => message = Some(m).filter(|m| !m.is_empty()),
            },
            Key::F(15) => {
                if lists::select_view(surface, st).is_none() {
                    return Exit::Gone;
                }
            }
            Key::F(16) => {
                let effect = st.session.apply(
                    Command::Find {
                        what: String::new(),
                        back: false,
                    },
                    cursor_line,
                );
                message = Some(match effect {
                    Ok(Effect::Message(m)) => m,
                    Ok(_) => String::new(),
                    Err(e) => e.to_string(),
                })
                .filter(|m| !m.is_empty());
            }
            Key::F(17) => match (stopped, cursor_word) {
                (false, _) => message = Some("The program is not stopped; nothing to watch.".into()),
                (true, None) => message = Some("Put the cursor on a variable first.".into()),
                (true, Some(w)) => {
                    let r = st.session.apply(Command::Watch { expr: w, len: None }, cursor_line);
                    message = Some(match r {
                        Ok(Effect::Message(m)) => m,
                        Ok(_) => String::new(),
                        Err(e) => e.to_string(),
                    });
                    super::remember_watches(job, st);
                }
            },
            Key::F(18) => {
                if lists::wrkwatch(job, host, surface, st).is_none() {
                    return Exit::Gone;
                }
            }
            Key::F(19) => {
                if let Some(v) = st.session.view.as_mut() {
                    v.left(Amount::Default);
                }
            }
            Key::F(20) => {
                if let Some(v) = st.session.view.as_mut() {
                    v.right(Amount::Default);
                }
            }
            Key::F(21) => {
                if lists::command_entry(job, host, surface, &screen).is_none() {
                    return Exit::Gone;
                }
            }
            Key::F(23) => {
                if lists::output(surface, st).is_none() {
                    return Exit::Gone;
                }
            }
            Key::F(24) => st.keys = (st.keys + 1) % 3,
            Key::Help | Key::F(1) => {
                if lists::help(surface, true).is_none() {
                    return Exit::Gone;
                }
            }
            Key::PageDown => {
                let at_bottom = st.session.view.as_ref().is_none_or(|v| v.is_empty() || v.at_bottom());
                if at_bottom {
                    message = Some("The last page is already shown.".into());
                } else {
                    st.session.key_scroll(true);
                }
            }
            Key::PageUp => {
                let at_top = st.session.view.as_ref().is_none_or(|v| v.top <= 1);
                if at_top {
                    message = Some("The first page is already shown.".into());
                } else {
                    st.session.key_scroll(false);
                }
            }
            Key::F(_) | Key::Other => message = Some("That key does nothing here.".into()),
        }
    }
}

/// What a command's effect does; `Some` leaves the display.
fn effect_of(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    st: &mut State,
    effect: Effect,
    typed: &str,
    message: &mut Option<String>,
) -> Option<Exit> {
    match effect {
        Effect::None => {}
        Effect::Message(m) => *message = Some(m),
        Effect::Resume { step, count, into } => {
            st.steps_left = if step { count } else { 0 };
            return Some(Exit::Resume(match (step, into) {
                (false, _) => Resume::Go,
                (true, false) => Resume::StepOver,
                (true, true) => Resume::StepInto,
            }));
        }
        Effect::Eval {
            expr,
            assign,
            format,
            len,
        } => {
            if assign.is_some() {
                *message = Some("Assigning to a variable is not supported yet.".into());
                return None;
            }
            let suffix = match format {
                Format::Natural => String::new(),
                Format::Hex => ":x".into(),
                Format::Char => ":c".into(),
            };
            let _ = len;
            let text = if st.serving.is_some() {
                remote::eval(host, &format!("{expr}{suffix}"))
            } else {
                job.eval(&format!("{expr}{suffix}"))
            };
            match text {
                Ok(v) if !v.contains('\n') && v.len() + expr.len() < 70 => {
                    let line = format!("{expr} = {v}");
                    st.evals.extend(dbg::screen::eval::history_entry(typed, &line));
                    *message = Some(line);
                }
                Ok(v) => {
                    st.evals.extend(dbg::screen::eval::history_entry(typed, &v));
                    if lists::evaluate(surface, st).is_none() {
                        return Some(Exit::Gone);
                    }
                }
                Err(e) => *message = Some(e),
            }
        }
        Effect::Attr(v) => {
            let name = v.trim_start_matches('&');
            *message = Some(
                job.dspvar()
                    .into_iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map_or_else(
                        || format!("{v} is not a variable of the stopped procedure."),
                        |(n, t)| format!("{n}: {t}"),
                    ),
            );
        }
        Effect::ShowModule(name) => {
            if !st.session.show_named(&name) {
                *message = Some(format!("Module {name} is not in the module list."));
            }
        }
        Effect::Help => {
            if lists::help(surface, false).is_none() {
                return Some(Exit::Gone);
            }
        }
        Effect::Threads => {
            let stopped = st.session.stopped.is_some();
            if lists::threads(job, host, surface, stopped).is_none() {
                return Some(Exit::Gone);
            }
        }
        Effect::WatchList => {
            if lists::wrkwatch(job, host, surface, st).is_none() {
                return Some(Exit::Gone);
            }
        }
        Effect::UpdProd(v) => {
            if st.serving.is_none() {
                job.debug
                    .get_or_insert_with(mi::debug::Debugger::default)
                    .updprod = v;
            }
            *message = Some(if v {
                "Production files may now be changed.".into()
            } else {
                "Production files may no longer be changed.".into()
            });
        }
    }
    None
}

/// A command that failed stays on the line, its first word in upper
/// case.
fn failed(text: &str) -> String {
    let t = text.trim();
    match t.split_once(' ') {
        Some((w, rest)) => format!("{} {rest}", w.to_ascii_uppercase()),
        None => t.to_ascii_uppercase(),
    }
}

/// The message a stop shows.
fn stop_text(st: &State, stmt: u32, reason: dbg::session::StopReason) -> String {
    let line = st
        .session
        .module()
        .and_then(|m| m.map.line_of(stmt))
        .map_or_else(|| format!("statement {stmt}"), |l| format!("line {l}"));
    let program = st
        .session
        .stopped
        .as_ref()
        .map(|s| s.program.clone())
        .unwrap_or_default();
    match reason {
        dbg::session::StopReason::Breakpoint => format!("Stopped at the breakpoint on {line} of {program}."),
        dbg::session::StopReason::Step => format!("Step done: stopped before {line} of {program}."),
        dbg::session::StopReason::Watch(n) => format!("Watch {n} changed; stopped before {line} of {program}."),
    }
}

/// `msg`, or the no-source message when the module has no source.
fn no_source_or(msg: &str, st: &State) -> String {
    if st.session.view.as_ref().is_none_or(dbg::SourceView::is_empty) {
        "The module has no source view; that needs one.".into()
    } else {
        msg.to_string()
    }
}
