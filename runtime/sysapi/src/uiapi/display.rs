// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Showing and printing: `QUIDSPP` runs the dialog of a panel until
//! the operator returns to the program (a `RETURN n` action, F3, F12,
//! paging past an incomplete list); options whose action is a command
//! and commands typed on the command line run on the way, as UIM does
//! for a list it acts on itself. `QUISETSA` places the cursor,
//! `QUIPRTP` prints, `QUIDSPH`, `QUHDSPH` and `QUHPRTH` are help.

use mi::Job;
use uim::engine::{Event, Response};
use uim::model::Action;

use super::{ApiTerm, App, Kind, SharedUimHost, fail, put, take, with_app};
use crate::param::{Args, ErrCode};
use crate::provider::ApiFail;

/// The function requested for `EXIT` (F3).
pub const FUNCTION_EXIT: i32 = -4;
/// The function requested for `CANCEL` (F12).
pub const FUNCTION_CANCEL: i32 = -8;
/// The function requested when the operator paged past the last entry
/// of an incomplete list: the program adds entries and shows the
/// panel again (`QUIGETLA` tells which list wants them by its
/// contents, `MORE`).
pub const FUNCTION_EXTEND: i32 = -6;

/// Run a command an action or the command line gives, through the
/// job's command hook (a leading `?` prompts it).
fn run_command(j: &mut Job, text: &str) -> Result<(), ApiFail> {
    let mode = if text.trim_start().starts_with('?') {
        2
    } else {
        0
    };
    match crate::provider_of(j) {
        Some(p) => {
            let mut p = p.borrow_mut();
            crate::misc::command(j, &mut *p, text, mode).map(|_| ())
        }
        None => {
            let mut p = crate::provider::MemProvider::default();
            crate::misc::command(j, &mut p, text, mode).map(|_| ())
        }
    }
}

/// The message a failed command leaves on the next display.
fn failed(e: &ApiFail) -> (String, bool) {
    (format!("{} ended the command.", e.msgid), true)
}

/// `QUIDSPP(handle, function requested out, panel, redisplay option,
/// errc)`: show the panel; the function requested is the `RETURN`
/// number, -4 for exit, -8 for cancel, -6 for more entries.
pub(super) fn quidspp(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let panel = a.char(2, 10).to_ascii_uppercase();
    let redisplay = a.char(3, 1);
    let mut app = match take(j, &a.bytes(0, 8)) {
        Ok(app) => app,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let r = if !matches!(redisplay.as_str(), "Y" | "N" | "") {
        Err(fail("CPF6A4B", Some(&app), &panel, ""))
    } else if !matches!(app.kind, Kind::Display) {
        Err(fail("CPF6A3E", Some(&app), &panel, ""))
    } else if app.engine.group.panel(&panel).is_none() {
        Err(fail("CPF6A3F", Some(&app), &panel, ""))
    } else {
        dialog(j, host, &mut app, &panel)
    };
    put(j, app);
    match r {
        Ok(n) => {
            a.put_bin4(1, n);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// The dialog of one `QUIDSPP`: the function requested.
fn dialog(j: &mut Job, host: &SharedUimHost, app: &mut App, panel: &str) -> Result<i32, ApiFail> {
    let Some(mut inner) = host.borrow_mut().terminal() else {
        return Err(fail("CPF6A1E", Some(app), panel, ""));
    };
    let mut current = panel.to_string();
    loop {
        let event = {
            let mut t = ApiTerm {
                inner: &mut *inner,
                host: host.clone(),
                job: j,
            };
            app.engine.show_event(&mut t, &current)
        };
        let event = event.map_err(|_| fail("CPF6A3F", Some(app), &current, ""))?;
        let response = match event {
            Event::More(_) => return Ok(FUNCTION_EXTEND),
            Event::Response(r) => r,
        };
        match response {
            Response::Return(n) => return Ok(i32::try_from(n).unwrap_or(i32::MAX)),
            Response::Exit => return Ok(FUNCTION_EXIT),
            Response::Cancel => return Ok(FUNCTION_CANCEL),
            Response::Gone => return Err(fail("CPF6A50", Some(app), &current, "")),
            Response::Panel(p) => current = p,
            Response::Command(c) => {
                if let Err(e) = run_command(j, &c) {
                    app.engine.message = Some(failed(&e));
                }
            }
            Response::Menu(m) => {
                if let Err(e) = run_command(j, &format!("GO MENU({m})")) {
                    app.engine.message = Some(failed(&e));
                }
            }
            Response::Retrieve => {
                app.engine.message = Some(("There is no command to retrieve.".into(), true));
            }
            Response::Print => {
                app.engine.message =
                    Some(("Printing is not available for this display.".into(), true));
            }
            Response::Options(choices) => {
                let mut ret = None;
                for c in &choices {
                    match &c.action {
                        Some(Action::Cmd(text)) => {
                            let cmd = app.engine.substitute(text, Some((&c.list, c.row)));
                            if let Err(e) = run_command(j, &cmd) {
                                app.engine.message = Some(failed(&e));
                            }
                            app.engine.list_mut(&c.list).options.remove(&c.row);
                        }
                        Some(Action::Return(n)) => ret = Some(*n),
                        // Anything else is the program's to act on.
                        _ => ret = ret.or(Some(0)),
                    }
                }
                if let Some(n) = ret {
                    return Ok(i32::try_from(n).unwrap_or(i32::MAX));
                }
            }
        }
    }
}

/// `QUISETSA(handle, variable, errc)`: the next `QUIDSPP` puts the
/// cursor on the field of the dialog variable.
pub(super) fn quisetsa(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let var = a.char(1, 10).to_ascii_uppercase();
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        if app.engine.group.variable(&var).is_none() {
            return Err(fail("CPF6A99", Some(app), "", &var));
        }
        app.engine.cursor_field = Some(var.clone());
        Ok(())
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIPRTP(handle, print panel, eject option, errc)`: a print
/// application prints the `:PRTPNL.` (spooled at close); a display
/// application prints the display panel as it would show now to
/// `QSYSPRT`.
pub(super) fn quiprtp(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let name = a.char(1, 10).to_ascii_uppercase();
    let eject = a.char(2, 1);
    let r = with_app(j, &a.bytes(0, 8), |j, app| {
        if !matches!(eject.as_str(), "Y" | "N" | "") {
            return Err(fail("CPF6A4C", Some(app), &name, ""));
        }
        match &mut app.kind {
            Kind::Print { lines, .. } => {
                let opts = uim::Options {
                    wide: true,
                    ..uim::Options::default()
                };
                let out = uim::render::render_print(&app.engine.group, &name, &app.engine, &opts)
                    .map_err(|_| fail("CPF6A3F", None, &name, ""))?;
                if eject == "Y" && !lines.is_empty() {
                    lines.push("\u{c}".into());
                }
                lines.extend(out);
                Ok(())
            }
            Kind::Display => {
                if app.engine.group.panel(&name).is_none() {
                    return Err(fail("CPF6A3F", Some(app), &name, ""));
                }
                let rows = app
                    .engine
                    .image(&name, false)
                    .map_err(|_| fail("CPF6A3F", None, &name, ""))?;
                j.spool_text("QSYSPRT", &rows.join("\n"))
                    .map(|_| ())
                    .map_err(|_| fail("CPF6A1B", None, &name, ""))
            }
        }
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// Help identifiers: each 52 bytes, the panel group and its library
/// (20) and the module (32), as `LIB/GROUP/MODULE`.
fn help_ids(a: &Args<'_>, i: usize, n: usize) -> Vec<String> {
    let raw = a.bytes(i, n * 52);
    raw.chunks(52)
        .filter(|c| c.len() == 52)
        .map(|c| {
            let g = ccsid::decode_trimmed(&c[..10]);
            let l = ccsid::decode_trimmed(&c[10..20]);
            let m = ccsid::decode_trimmed(&c[20..]);
            let l = if l.is_empty() { "*LIBL".to_string() } else { l };
            format!("{l}/{g}/{m}")
        })
        .collect()
}

/// The number of help identifiers, or `CPF6E0A`.
fn help_count(a: &Args<'_>, i: usize) -> Result<usize, ApiFail> {
    usize::try_from(a.bin4(i))
        .ok()
        .filter(|n| (1..=1000).contains(n))
        .ok_or_else(|| ApiFail::id("CPF6E0A"))
}

/// Show help with an engine on the host's display.
fn show_help(
    j: &mut Job,
    host: &SharedUimHost,
    engine: &mut uim::engine::Engine,
    ids: &[String],
    extended: bool,
    full: bool,
) -> Result<(), ApiFail> {
    let Some(mut inner) = host.borrow_mut().terminal() else {
        return Err(ApiFail::id("CPF6E3B"));
    };
    let mut t = ApiTerm {
        inner: &mut *inner,
        host: host.clone(),
        job: j,
    };
    match engine.show_help(&mut t, ids, extended, full) {
        Some(true) | None => Ok(()),
        Some(false) => Err(ApiFail::id("CPF6E3B")),
    }
}

/// The help type: `0` field help, `1` extended help.
fn help_type(s: &str) -> Result<bool, ApiFail> {
    match s {
        "0" | "" => Ok(false),
        "1" => Ok(true),
        _ => Err(ApiFail::id("CPF6E0A")),
    }
}

/// `QUIDSPH(handle, help identifiers, number of identifiers, help type,
/// full-screen help, errc)`: help within an application.
pub(super) fn quidsph(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let r = help_count(&a, 2).and_then(|n| {
        let ids = help_ids(&a, 1, n);
        let extended = help_type(&a.char(3, 1))?;
        let full = a.char(4, 1) == "Y";
        with_app(j, &a.bytes(0, 8), |j, app| {
            let full = full || app.engine.full_screen_help;
            show_help(j, host, &mut app.engine, &ids, extended, full)
        })
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUHDSPH(help identifiers, number of identifiers, help type, search
/// index, display type, upper-left corner, lower-right corner, cursor
/// location, errc)`: help without an application; display type `Y`
/// is full screen, `N` a window.
pub(super) fn quhdsph(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let r = help_count(&a, 1).and_then(|n| {
        let ids = help_ids(&a, 0, n);
        let extended = help_type(&a.char(2, 1))?;
        let full = a.char(4, 1) != "N";
        let system = host.borrow_mut().system_name();
        let mut engine =
            uim::engine::Engine::new(std::rc::Rc::new(uim::PanelGroup::default()), &system);
        let (sl, si) = a.qual(3);
        if !si.is_empty() && si != "*NONE" {
            let g = uim::PanelGroup {
                schidx: Some(if sl.is_empty() {
                    si
                } else {
                    format!("{sl}/{si}")
                }),
                ..uim::PanelGroup::default()
            };
            engine.group = std::rc::Rc::new(g);
        }
        show_help(j, host, &mut engine, &ids, extended, full)
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUHPRTH(help identifiers, number of identifiers, errc)`: print the
/// help to `QSYSPRT`.
pub(super) fn quhprth(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let r = help_count(&a, 1).and_then(|n| {
        let ids = help_ids(&a, 0, n);
        let mut engine =
            uim::engine::Engine::new(std::rc::Rc::new(uim::PanelGroup::default()), "QS400");
        let lines = {
            let mut none = NoDisplay(host.clone());
            engine.help_text(&mut none, &ids)
        }
        .ok_or_else(|| ApiFail::id("CPF6E3B"))?;
        j.spool_text("QSYSPRT", &lines.join("\n"))
            .map(|_| ())
            .map_err(|_| ApiFail::id("CPF6E3B"))
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// A terminal that shows nothing but opens panel groups: help text
/// for printing needs the groups only.
struct NoDisplay(SharedUimHost);

impl uim::engine::Terminal for NoDisplay {
    fn wide(&mut self) -> bool {
        false
    }

    fn exchange(&mut self, _screen: &uim::Screen) -> Option<uim::engine::Input> {
        None
    }

    fn group(&mut self, lib: &str, name: &str) -> Option<std::rc::Rc<uim::PanelGroup>> {
        self.0
            .borrow_mut()
            .panel_group(lib, name)
            .ok()
            .map(|(g, _)| g)
    }
}
