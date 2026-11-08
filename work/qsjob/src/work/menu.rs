// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Menus for the interactive job: `GO`, the menu stack, a `*UIM` menu's
//! panel shown through the UIM engine, its options run as commands or
//! as other menus, `F3`, `F12`, `F16`, `F23` and the sign-off option.

use std::rc::Rc;

use menus::{MenuAttrs, MenuKind, MenuStack, Outcome, Qualified, Target};
use stg::Client;
use wm::job::Status;

use super::inter::{self, Session};
use crate::command::RunMode;
use crate::display::surface::Surface;
use crate::display::uim::{LineMsg, Response, UimDialog};

/// How a menu session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Back to where `GO` was typed.
    Back,
    /// The user signed off.
    SignOff,
    /// The display is gone or the job is to end.
    End,
}

/// The menu and `RTNPNT` of `GO`'s parameters: `text` is what
/// follows the command name.
fn go_params(text: &str) -> (String, bool) {
    // The parser skips a command name; GO's is put back in front.
    let p = wm::cmd::Params::parse(&format!("GO {text}"));
    let menu = p
        .pos_or(0, "MENU")
        .unwrap_or_else(|_| "MAIN".into())
        .to_ascii_uppercase();
    (menu, p.upper("RTNPNT").as_deref() != Some("*NO"))
}

/// `GO MENU(...)` typed: `text` is what follows the command name.
pub(super) fn go(s: &mut Session<'_>, text: &str, initial: bool) -> Flow {
    let (menu, rtnpnt) = go_params(text);
    let target = match menu.as_str() {
        "*MAIN" => Target::Main,
        "*USER" => Target::User,
        "*SIGNOFF" => Target::SignOff,
        _ => {
            let (lib, name) = menu
                .rsplit_once('/')
                .map_or(("*LIBL", menu.as_str()), |(l, n)| (l, n));
            if name.ends_with('*') {
                let found = s
                    .link
                    .borrow_mut()
                    .with(|l| l.list_objects(lib.into(), name.into(), "*MENU".into()));
                Target::Choice(
                    found
                        .into_iter()
                        .map(|o| Qualified::new(&o.lib, &o.name))
                        .collect(),
                )
            } else {
                Target::Menu(Qualified::new(lib, name))
            }
        }
    };
    let user_menu = match s.signon.inlmnu.as_str() {
        "*SIGNOFF" | "" => None,
        m => Some(Qualified::new("*LIBL", m)),
    };
    let first = match target {
        Target::Menu(q) => q,
        Target::Main => Qualified::qsys("MAIN"),
        Target::User => match user_menu.clone() {
            Some(q) => q,
            None => return Flow::SignOff,
        },
        Target::SignOff => return Flow::SignOff,
        Target::Choice(list) => match list.len() {
            0 => {
                s.say(LineMsg::error("CPD9BB4", &[&menu]));
                return Flow::Back;
            }
            1 => list[0].clone(),
            _ => {
                s.say(LineMsg::error("CPD9BB5", &[&menu]));
                return Flow::Back;
            }
        },
    };
    let mut stack = MenuStack::new(first, user_menu);
    let _ = rtnpnt;
    show_stack(s, &mut stack, initial)
}

/// Show the stack's current menu until it is left.
#[allow(clippy::too_many_lines)] // one arm per response
fn show_stack(s: &mut Session<'_>, stack: &mut MenuStack, initial: bool) -> Flow {
    let mut surface = Surface::new(s.c.host.clone());
    let mut message: Option<LineMsg> = None;
    loop {
        let cur = stack.current().clone();
        // Only a missing *MENU is CPF9801; anything else says why.
        let opened = open(s, &cur);
        let left = match &opened {
            Ok((attrs, real)) => match attrs.kind {
                MenuKind::Uim => None,
                MenuKind::Pgm => Some(pgm_menu(s, &cur, real, attrs)),
                MenuKind::Dspf => Some(dspf_menu(s, &cur, real, attrs, message.take())),
            },
            Err(m) => Some(Left::Failed(m.clone())),
        };
        if let Some(left) = left {
            match left {
                Left::Exit => {
                    if stack.exit().is_none() {
                        return if initial { Flow::SignOff } else { Flow::Back };
                    }
                }
                Left::Cancel => {
                    if stack.cancel().is_none() {
                        return if initial { Flow::SignOff } else { Flow::Back };
                    }
                }
                Left::Home => {
                    let _ = stack.exit();
                }
                Left::Flow(f) => return f,
                Left::Failed(m) => {
                    if stack.depth() <= 1 {
                        s.say(m);
                        return Flow::Back;
                    }
                    stack.cancel();
                    message = Some(m);
                }
            }
            continue;
        }
        let Ok((attrs, real)) = opened else {
            continue;
        };
        let group = match load_pnlgrp(s, &cur, &real, &attrs) {
            Ok(g) => g,
            Err(m) => {
                if stack.depth() <= 1 {
                    s.say(m);
                    return Flow::Back;
                }
                stack.cancel();
                message = Some(m);
                continue;
            }
        };
        let mut dialog = UimDialog::new(Rc::clone(&group), &s.system);
        dialog.copyright = true;
        if let Some(m) = message.take() {
            dialog.send(m);
        }
        inter::status_of(s, Status::Dspw, &format!("MNU-{}", cur.name));
        let panel = cur.name.clone();
        loop {
            let r = match dialog.show(&mut surface, &panel) {
                Ok(r) => r,
                Err(e) => {
                    s.say(crate::panels::line(&format!("menu {}: {e}", cur.name)));
                    return Flow::Back;
                }
            };
            match r {
                // A request System Request ended: back where it began.
                Response::Gone if crate::sysdsp::keys::cancelling() => return Flow::Back,
                Response::Gone => return Flow::End,
                Response::Exit => {
                    // F3 leaves the first menu: the session's initial
                    // menu signs off, a menu reached by GO returns.
                    if stack.exit().is_none() {
                        return if initial { Flow::SignOff } else { Flow::Back };
                    }
                    break;
                }
                Response::Cancel => {
                    if stack.cancel().is_none() {
                        return if initial { Flow::SignOff } else { Flow::Back };
                    }
                    break;
                }
                Response::Menu(name) => {
                    if let Some(id) = menus::unavailable::menu(&name) {
                        dialog.error(id, &[&name]);
                        continue;
                    }
                    let target = match name.as_str() {
                        "MAIN" => Target::Main,
                        n => Target::Menu(Qualified::new("*LIBL", n)),
                    };
                    match stack.go(target, true) {
                        Outcome::Show(_) => break,
                        Outcome::Choose(_) | Outcome::NotFound => {
                            dialog.error("CPF9801", &[&name, "*LIBL"]);
                        }
                        Outcome::SignOff => return Flow::SignOff,
                    }
                }
                Response::Command(text) => {
                    if let Some((id, name)) = unavailable(&group, &panel, &text) {
                        dialog.error(id, &[&name]);
                        continue;
                    }
                    let text = dialog.substitute(&text, None);
                    match run(s, &text, &mut dialog) {
                        Flow::Back => {}
                        other => return other,
                    }
                }
                Response::Retrieve => {
                    let prev = inter::retrieve(s);
                    dialog.set(crate::display::uim::CMDLINE_VAR, &prev);
                }
                Response::Return(23) => {
                    let who = s.signon.user.clone();
                    let m = inter::command_msg(
                        s,
                        &format!("CHGUSRPRF USRPRF({who}) INLMNU({}/{})", cur.lib, cur.name),
                        RunMode::Run,
                    );
                    match m {
                        Some(m) if m.error => dialog.send(m),
                        _ => dialog.info("CPI9BB9", &[&cur.name]),
                    }
                }
                Response::Return(_) | Response::Options(_) | Response::Panel(_) => {}
                Response::Print => {
                    dialog.error("CPD9B84", &[]);
                }
            }
        }
    }
}

/// A `*DSPF` menu (pub400): the display file's record named like the
/// file, `===>` at row 22 and the command line from 22,7 (153 positions
/// for `CMDLIN(*LONG)`, 73 for `*SHORT`, 4 for `*NONE`); a number runs
/// the text of message `USRnnnn` of the menu's message file, anything
/// else runs as a command; `F3` leaves, `F12` goes back, `F9` retrieves.
#[allow(clippy::too_many_lines)] // one arm per key
fn dspf_menu(
    s: &mut Session<'_>,
    menu: &Qualified,
    real: &str,
    attrs: &MenuAttrs,
    mut message: Option<LineMsg>,
) -> Left {
    use tn5250::ds::{aid, attr, field::Ffw};
    let file = part(attrs.dspf.as_ref(), menu, real);
    let msgf = part(attrs.msgf.as_ref(), menu, real);
    let opened = s
        .link
        .borrow_mut()
        .with(|l| l.dsp_open(file.name.clone(), file.lib.clone(), "*JOB".into()));
    let df = match opened.map(|(b, _)| dspf::decode(&b)) {
        Ok(Ok(df)) => df,
        Ok(Err(e)) => return Left::Failed(crate::panels::line(&format!("CPF4101 {e}"))),
        Err(e) => return Left::Failed(crate::panels::line(&e)),
    };
    let Some(rec) = df.record(&file.name).cloned() else {
        return Left::Failed(LineMsg::error("CPF9801", &[&file.name, &file.lib]));
    };
    let len = match attrs.cmdlin {
        menus::CmdLin::Long => 153,
        menus::CmdLin::Short => 73,
        menus::CmdLin::None => 4,
    };
    let host = s.c.host.clone();
    let mut surface = Surface::new(host.clone());
    let wide = surface.wide();
    let mut rt = dspf::rt::Runtime::new(df, wide);
    let sysname = crate::display::dds::system_name(&host);
    let env = crate::display::dds::env_of(&host, &sysname);
    let mut line = String::new();
    inter::status_of(s, Status::Dspw, &format!("MNU-{}", menu.name));
    loop {
        let out = crate::display::dds::out_buffer(&rec, &[]);
        let shown = surface.with_model(|m| {
            m.clear();
            let r = rt.write(m, &env, &rec.name, &out, &[]);
            m.put(22, 2, "===>", attr::GREEN);
            m.field(22, 7, len, attr::GREEN_UL, &line, Ffw::default(), &[]);
            m.set_cursor(22, 7);
            if let Some(msg) = message.take() {
                m.error_line(&crate::display::msgline::text_of(&host, &msg), attr::WHITE);
                m.alarm();
            }
            r
        });
        if shown.is_err() {
            return Left::Failed(LineMsg::error("CPF4101", &[&file.name, &file.lib]));
        }
        surface.flush(tn5250::screen::Read::MdtFields);
        let Some((a, _)) = surface.wait() else {
            return Left::Flow(Flow::End);
        };
        let typed = surface.with_model(|m| {
            m.fields()
                .iter()
                .find(|f| f.row == 22 && f.col == 7)
                .map(tn5250::screen::Field::text)
                .unwrap_or_default()
        });
        line.clear();
        match aid::function_of(a) {
            Some(3) => return Left::Exit,
            Some(12) => return Left::Cancel,
            Some(9) => {
                line = inter::retrieve(s);
                continue;
            }
            Some(_) => {
                message = Some(LineMsg::error("CPD6A60", &[]));
                continue;
            }
            None if a != aid::ENTER => {
                message = Some(LineMsg::error("CPD6A60", &[]));
                continue;
            }
            None => {}
        }
        let typed = typed.trim().to_string();
        let command = if !typed.is_empty() && typed.chars().all(|c| c.is_ascii_digit()) {
            let id = typed
                .parse::<u16>()
                .ok()
                .map(|n| format!("USR{n:04}"))
                .unwrap_or_default();
            let found = s
                .link
                .borrow_mut()
                .with(|l| l.describe(id, msgf.name.clone(), msgf.lib.clone()));
            let Some(d) = found else {
                message = Some(LineMsg::error("CPD9BAB", &[&typed]));
                line = typed;
                continue;
            };
            d.format(&d.data_of::<&str>(&[]))
        } else {
            typed
        };
        let (flow, said, keep) = run_line(s, &command);
        if flow != Flow::Back {
            return Left::Flow(flow);
        }
        if keep {
            line = command;
        }
        message = said;
    }
}

/// Run a command typed on a `*UIM` menu.
fn run(s: &mut Session<'_>, text: &str, dialog: &mut UimDialog) -> Flow {
    let (flow, said, keep) = run_line(s, text);
    if let Some(m) = said {
        dialog.send(m);
    }
    if keep {
        // The command stays on the line to be corrected.
        dialog.set(crate::display::uim::CMDLINE_VAR, text.trim());
    }
    flow
}

/// Run a command typed on a menu, a menu's own words first: where the
/// session goes, the message to show and whether the line keeps the
/// command (a `GO` that failed).
fn run_line(s: &mut Session<'_>, text: &str) -> (Flow, Option<LineMsg>, bool) {
    let typed = text.trim();
    if typed.is_empty() {
        return (Flow::Back, None, false);
    }
    let word = typed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('?')
        .to_ascii_lowercase();
    match word.as_str() {
        "signoff" => {
            inter::remember(s, typed);
            inter::signoff(s, typed);
            (Flow::SignOff, None, false)
        }
        "go" => {
            inter::remember(s, typed);
            let rest = typed.split_once(char::is_whitespace).map_or("", |(_, r)| r);
            let _request = crate::sysdsp::keys::Request::enter();
            let flow = go(s, rest, false);
            // What GO had to say (a menu not found) shows on this menu.
            let said = s.take_message();
            let keep = said.is_some();
            (flow, said, keep)
        }
        _ => {
            let prompt = typed.starts_with('?');
            let mode = if prompt {
                RunMode::PromptRun
            } else {
                RunMode::Run
            };
            let cmd = typed.trim_start_matches('?');
            inter::remember(s, cmd);
            if word == "call" && cmd.to_ascii_uppercase().trim_end().ends_with("QCMD") {
                let flow = if inter::command_entry(s) {
                    Flow::SignOff
                } else {
                    Flow::Back
                };
                return (flow, None, false);
            }
            (Flow::Back, inter::command_msg(s, cmd, mode), false)
        }
    }
}

/// A command this release does not have: one left out on purpose
/// (`menus::unavailable`, chosen or typed), or a command menu's reserved
/// option (an option with no text). Any other goes to the analyzer.
fn unavailable(group: &uim::PanelGroup, panel: &str, text: &str) -> Option<(&'static str, String)> {
    if let Some(hit) = menus::unavailable::command(text) {
        return Some(hit);
    }
    let menu = group.panel(panel)?.areas.iter().find_map(|a| match a {
        uim::model::Area::Menu(m) => Some(m),
        _ => None,
    })?;
    menu.all_items()
        .into_iter()
        .any(|i| i.text.is_empty() && matches!(&i.action, uim::model::Action::Cmd(c) if c == text))
        .then(|| {
            (
                menus::unavailable::COMMAND,
                menus::unavailable::command_name(text),
            )
        })
}

/// `CPF9801`: the menu is not there (the oracle says so for GO).
fn not_found(q: &Qualified) -> LineMsg {
    LineMsg::error("CPF9801", &[&q.name, &q.lib])
}

/// How a `*PGM` or `*DSPF` menu was left.
enum Left {
    /// `F3`, or a program's return code -1.
    Exit,
    /// `F12`, or -2: the menu before.
    Cancel,
    /// -4: the first menu.
    Home,
    /// The session goes elsewhere.
    Flow(Flow),
    /// It could not be shown.
    Failed(LineMsg),
}

/// A menu's attributes and the library it was found in; `CPF9801` only
/// when the `*MENU` itself is missing.
fn open(s: &mut Session<'_>, q: &Qualified) -> Result<(MenuAttrs, String), LineMsg> {
    let (bytes, real) = s
        .link
        .borrow_mut()
        .with(|l| l.menu_open(q.name.clone(), q.lib.clone()))
        .map_err(|e| {
            if e.starts_with("CPF9801") {
                not_found(q)
            } else {
                crate::panels::line(&e)
            }
        })?;
    let attrs = MenuAttrs::decode(&bytes).map_err(|e| crate::panels::line(&e.to_string()))?;
    Ok((attrs, real))
}

/// A part a menu names (`*MENU` its own name or library, `real` the
/// library it was found in).
fn part(q: Option<&Qualified>, menu: &Qualified, real: &str) -> Qualified {
    let q = q
        .cloned()
        .unwrap_or_else(|| Qualified::new("*LIBL", "*MENU"));
    let name = if q.name == "*MENU" {
        menu.name.clone()
    } else {
        q.name
    };
    let lib = if q.lib == "*MENU" {
        real.to_string()
    } else {
        q.lib
    };
    Qualified { name, lib }
}

/// A `*UIM` menu's panel group through the storage service.
fn load_pnlgrp(
    s: &mut Session<'_>,
    q: &Qualified,
    real: &str,
    attrs: &MenuAttrs,
) -> Result<Rc<uim::PanelGroup>, LineMsg> {
    let pnlgrp = attrs
        .pnlgrp
        .clone()
        .unwrap_or_else(|| Qualified::new(real, &q.name));
    let pnlgrp = part(Some(&pnlgrp), q, real);
    let (payload, _) = s
        .link
        .borrow_mut()
        .with(|l| l.pnlgrp_open(pnlgrp.name.clone(), pnlgrp.lib.clone()))
        .map_err(|e| crate::panels::line(&e))?;
    let p = uim::payload::decode(&payload).map_err(|e| crate::panels::line(&e.to_string()))?;
    Ok(Rc::new(p.group))
}

/// Text in CP037, blank padded to `n`.
fn padded(text: &str, n: usize) -> Vec<u8> {
    let mut b = ccsid::to_ebcdic(text).unwrap_or_default();
    b.resize(n, 0x40);
    b
}

/// A `*PGM` menu: its program called with the menu's name, the library
/// it was found in and a 2-byte return code, again while it returns 0
/// (pub400: the library is the real one even when GO named `*LIBL`).
fn pgm_menu(s: &mut Session<'_>, menu: &Qualified, real: &str, attrs: &MenuAttrs) -> Left {
    let pgm = part(attrs.pgm.as_ref(), menu, real);
    let lib = if pgm.lib == "*LIBL" { "" } else { &pgm.lib };
    inter::status_of(s, Status::Run, &format!("MNU-{}", menu.name));
    loop {
        let mut parms = vec![padded(&menu.name, 10), padded(real, 10), vec![0, 0]];
        if let Err(e) = regfac::ProgramCaller::call(s.c.job(), &pgm.name, lib, &mut parms) {
            return Left::Failed(crate::panels::line(&e));
        }
        match i16::from_be_bytes([parms[2][0], parms[2][1]]) {
            0 => {}
            -2 => return Left::Cancel,
            -4 => return Left::Home,
            _ => return Left::Exit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::go_params;

    #[test]
    fn go_takes_the_menu_typed() {
        assert_eq!(go_params("USER"), ("USER".into(), true));
        assert_eq!(
            go_params("menu(qgpl/mine) rtnpnt(*no)"),
            ("QGPL/MINE".into(), false)
        );
        assert_eq!(go_params(""), ("MAIN".into(), true));
        assert_eq!(go_params("*main"), ("*MAIN".into(), true));
    }
}
