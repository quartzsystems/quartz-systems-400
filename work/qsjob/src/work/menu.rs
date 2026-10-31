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
        let loaded = load(s, &cur);
        let (attrs, group) = match loaded {
            Ok(x) => x,
            Err(e) => {
                let m = not_found(&cur);
                let _ = e;
                if stack.depth() <= 1 {
                    s.say(m);
                    return Flow::Back;
                }
                stack.cancel();
                message = Some(m);
                continue;
            }
        };
        if attrs.kind != MenuKind::Uim {
            s.say(LineMsg::error("CPD9BB6", &[&cur.name, attrs.kind.text()]));
            if stack.cancel().is_none() {
                return Flow::Back;
            }
            continue;
        }
        let mut dialog = UimDialog::new(group, &s.system);
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

/// Run a command typed on a menu; a menu's own words first.
fn run(s: &mut Session<'_>, text: &str, dialog: &mut UimDialog) -> Flow {
    let typed = text.trim();
    if typed.is_empty() {
        return Flow::Back;
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
            Flow::SignOff
        }
        "go" => {
            inter::remember(s, typed);
            let rest = typed.split_once(char::is_whitespace).map_or("", |(_, r)| r);
            let _request = crate::sysdsp::keys::Request::enter();
            let flow = go(s, rest, false);
            // What GO had to say (a menu not found) shows on this menu.
            if let Some(m) = s.take_message() {
                dialog.send(m);
                // The command stays on the line to be corrected.
                dialog.set(crate::display::uim::CMDLINE_VAR, typed);
            }
            flow
        }
        _ => {
            let prompt = typed.starts_with('?');
            let mode = if prompt { RunMode::PromptRun } else { RunMode::Run };
            let cmd = typed.trim_start_matches('?');
            inter::remember(s, cmd);
            if word == "call" && cmd.to_ascii_uppercase().trim_end().ends_with("QCMD") {
                return if inter::command_entry(s) { Flow::SignOff } else { Flow::Back };
            }
            if let Some(m) = inter::command_msg(s, cmd, mode) {
                dialog.send(m);
            }
            Flow::Back
        }
    }
}

/// `CPF9801`: the menu is not there (the oracle says so for GO).
fn not_found(q: &Qualified) -> LineMsg {
    LineMsg::error("CPF9801", &[&q.name, &q.lib])
}

/// A menu's attributes and panel group through the storage service.
fn load(s: &mut Session<'_>, q: &Qualified) -> Result<(MenuAttrs, Rc<uim::PanelGroup>), String> {
    let (bytes, real) = s
        .link
        .borrow_mut()
        .with(|l| l.menu_open(q.name.clone(), q.lib.clone()))?;
    let attrs = MenuAttrs::decode(&bytes).map_err(|e| e.to_string())?;
    let pnlgrp = attrs
        .pnlgrp
        .clone()
        .unwrap_or_else(|| Qualified::new(&real, &q.name));
    let lib = if pnlgrp.lib == "*MENU" { real.clone() } else { pnlgrp.lib.clone() };
    let (payload, _) = s
        .link
        .borrow_mut()
        .with(|l| l.pnlgrp_open(pnlgrp.name.clone(), lib))?;
    let p = uim::payload::decode(&payload).map_err(|e| e.to_string())?;
    Ok((attrs, Rc::new(p.group)))
}

#[cfg(test)]
mod tests {
    use super::go_params;

    #[test]
    fn go_takes_the_menu_typed() {
        assert_eq!(go_params("USER"), ("USER".into(), true));
        assert_eq!(go_params("menu(qgpl/mine) rtnpnt(*no)"), ("QGPL/MINE".into(), false));
        assert_eq!(go_params(""), ("MAIN".into(), true));
        assert_eq!(go_params("*main"), ("*MAIN".into(), true));
    }
}
