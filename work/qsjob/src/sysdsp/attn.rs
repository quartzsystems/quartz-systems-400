// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Attention key: the attention program runs at the System Request
//! level. `SETATNPGM` sets one for the job (a stack: `*PRV` goes back,
//! `SET(*OFF)` turns the key off); without one the profile's `ATNPGM`
//! runs, `*SYSVAL` meaning `QATNPGM`. `*ASSIST` shows the Operational
//! Assistant menu `ASSIST`, `QSYS/QUSCMDLN` a command-line window over
//! the screen, `*NONE` nothing; any other program is called.

use std::cell::RefCell;

use menus::MenuAttrs;
use stg::Client;
use tn5250::ds::{aid, attr, field::Ffw};
use tn5250::screen::Read;

use super::keys::{self, Act};
use crate::command::{Fail, RunMode};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

thread_local! {
    /// `SETATNPGM`: the programs set, the last in force, each with
    /// whether the key is on.
    static STACK: RefCell<Vec<(String, bool)>> = const { RefCell::new(Vec::new()) };
}

/// `SETATNPGM PGM(...) SET(*ON|*OFF)`.
#[allow(clippy::unnecessary_wraps)] // the commands' signature
pub fn setatnpgm(host: &Host, pgm: &str, set: &str) -> Result<String, Fail> {
    let on = set != "*OFF";
    STACK.with(|s| {
        let mut s = s.borrow_mut();
        match pgm {
            "*PRV" => {
                s.pop();
            }
            "*CURRENT" | "" => {
                let cur = s.last().map_or_else(|| profile_program(host), |(p, _)| p.clone());
                s.push((cur, on));
            }
            p => s.push((p.to_string(), on)),
        }
    });
    Ok(String::new())
}

/// The profile's attention program, `*SYSVAL` resolved.
fn profile_program(host: &Host) -> String {
    let p = host
        .start
        .signon
        .as_ref()
        .map_or_else(|| "*SYSVAL".to_string(), |s| s.atnpgm.clone());
    if p.is_empty() || p == "*SYSVAL" {
        return host
            .link
            .borrow_mut()
            .with(|l| l.sysval("QATNPGM".into()))
            .map_or_else(
                || "*ASSIST".to_string(),
                |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
            );
    }
    p
}

/// The attention program in force, `None` when the key is off.
#[must_use]
pub fn program(host: &Host) -> Option<String> {
    match STACK.with(|s| s.borrow().last().cloned()) {
        Some((_, false)) => None,
        Some((p, true)) => Some(p),
        None => Some(profile_program(host)),
    }
}

/// What a program value names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Nothing happens.
    Nothing,
    /// The Operational Assistant menu.
    Assist,
    /// The command-line window.
    CmdLine,
    /// Call a program, `lib/name`.
    Call(String),
}

/// The target of an attention program value (`lib/name` or a word;
/// a `QSYSVAL`'s 20 characters are the name then the library).
#[must_use]
pub fn target(value: &str) -> Target {
    let v = value.trim().to_ascii_uppercase();
    let (lib, name) = match v.split_once('/') {
        Some((l, n)) => (l.trim().to_string(), n.trim().to_string()),
        None if v.len() > 10 && !v.starts_with('*') => {
            (v[10..].trim().to_string(), v[..10].trim().to_string())
        }
        None => ("*LIBL".to_string(), v.clone()),
    };
    match name.as_str() {
        "*NONE" | "" => Target::Nothing,
        "*ASSIST" => Target::Assist,
        "QUSCMDLN" if matches!(lib.as_str(), "QSYS" | "*LIBL") => Target::CmdLine,
        _ => Target::Call(format!("{lib}/{name}")),
    }
}

/// The Attention key was pressed.
pub fn run(host: &Host) -> Act {
    let Some(p) = program(host) else {
        return Act::Back;
    };
    match target(&p) {
        Target::Nothing => Act::Back,
        Target::Assist => assist(host),
        Target::CmdLine => cmdline(host),
        Target::Call(q) => {
            let o = keys::command(host, &format!("CALL PGM({q})"), RunMode::Run);
            if keys::unwinding() {
                return Act::Gone;
            }
            match keys::message_of(&o) {
                Some(m) if o.escape.is_some() => Act::Say(m),
                _ => Act::Back,
            }
        }
    }
}

/// A menu's panel group through the storage service.
fn menu_group(host: &Host, name: &str) -> Option<std::rc::Rc<uim::PanelGroup>> {
    let (bytes, real) = host
        .link
        .borrow_mut()
        .with(|l| l.menu_open(name.into(), "*LIBL".into()))
        .ok()?;
    let attrs = MenuAttrs::decode(&bytes).ok()?;
    let q = attrs
        .pnlgrp
        .clone()
        .unwrap_or_else(|| menus::Qualified::new(&real, name));
    let lib = if q.lib == "*MENU" { real } else { q.lib.clone() };
    let (payload, _) = host
        .link
        .borrow_mut()
        .with(|l| l.pnlgrp_open(q.name.clone(), lib))
        .ok()?;
    uim::payload::decode(&payload)
        .ok()
        .map(|p| std::rc::Rc::new(p.group))
}

/// The Operational Assistant menu and the menus it leads to, until F3
/// or F12 on the first.
fn assist(host: &Host) -> Act {
    let system = crate::display::dds::system_name(host);
    let mut surface = Surface::new(host.clone());
    let mut stack = vec!["ASSIST".to_string()];
    let mut message: Option<String> = None;
    while let Some(cur) = stack.last().cloned() {
        let Some(g) = menu_group(host, &cur) else {
            if stack.len() == 1 {
                return Act::Say("The Operational Assistant menu ASSIST is not available.".into());
            }
            stack.pop();
            message = Some(format!("Menu {cur} not found."));
            continue;
        };
        let mut d = UimDialog::new(g, &system);
        d.copyright = true;
        loop {
            if let Some(m) = message.take() {
                d.message = Some((m, true));
            }
            let r = match d.show(&mut surface, &cur) {
                Ok(r) => r,
                Err(e) => {
                    message = Some(format!("Menu {cur}: {e}"));
                    stack.pop();
                    break;
                }
            };
            match r {
                Response::Gone => return Act::Gone,
                Response::Exit => return Act::Back,
                Response::Cancel => {
                    stack.pop();
                    break;
                }
                Response::Menu(name) => {
                    stack.push(name);
                    break;
                }
                Response::Command(text) => {
                    let text = d.substitute(&text, None);
                    let prompt = text.starts_with('?');
                    let mode = if prompt { RunMode::PromptRun } else { RunMode::Run };
                    let o = keys::command(host, text.trim_start_matches('?'), mode);
                    if keys::unwinding() {
                        return Act::Gone;
                    }
                    if let Some(m) = keys::message_of(&o) {
                        message = Some(m);
                    }
                }
                _ => {}
            }
        }
    }
    Act::Back
}

/// The first row of the command-line window, its rows and width.
const WIN_TOP: usize = 17;
const WIN_ROWS: usize = 6;

/// Draw the command-line window over the screen; the field's index.
fn draw_window(m: &mut tn5250::screen::ScreenModel, value: &str, message: &str) -> Option<usize> {
    let cols = m.cols();
    let right = cols - 1;
    let inner = right - 3;
    for i in 0..m.fields().len() {
        m.set_bypass(i, true);
    }
    m.clear_rows(WIN_TOP, WIN_TOP + WIN_ROWS - 1);
    let dots = ".".repeat(right - 2);
    m.put(WIN_TOP, 2, &dots, attr::BLUE);
    for r in WIN_TOP + 1..WIN_TOP + WIN_ROWS - 1 {
        m.put(r, 2, ":", attr::BLUE);
        m.put(r, right, ":", attr::BLUE);
    }
    m.put(WIN_TOP + WIN_ROWS - 1, 2, &format!(":{}:", ".".repeat(right - 4)), attr::BLUE);
    m.put(WIN_TOP + 2, 4, "===>", attr::GREEN);
    let len = inner - 8;
    m.field(WIN_TOP + 2, 9, len, attr::GREEN_UL, value, Ffw::default(), &[]);
    if !message.is_empty() {
        let shown: String = message.chars().take(right - 6).collect();
        m.put(WIN_TOP + 3, 4, &shown, attr::WHITE);
    }
    m.put(WIN_TOP + 4, 4, "F4=Prompt   F9=Retrieve   F12=Cancel", attr::BLUE);
    m.set_cursor(WIN_TOP + 2, 9);
    m.field_index_at(WIN_TOP + 2, 9)
}

/// `QUSCMDLN`: a command line in a window over the screen, until F12
/// (or F3) leaves it.
pub fn cmdline(host: &Host) -> Act {
    let saved = host.display.borrow().as_ref().map(crate::display::Display::snapshot);
    let mut surface = Surface::new(host.clone());
    let mut typed = String::new();
    let mut message = String::new();
    let mut previous: Vec<String> = Vec::new();
    let mut back = 0usize;
    loop {
        if let Some(s) = saved.clone()
            && let Some(d) = host.display.borrow_mut().as_mut()
        {
            d.restore_quiet(s);
        }
        let field = surface.with_model(|m| draw_window(m, &typed, &message));
        surface.flush(Read::MdtFields);
        let Some((a, _)) = surface.wait() else {
            return Act::Gone;
        };
        let value = surface.with_model(|m| {
            field
                .and_then(|i| m.fields().get(i).map(tn5250::screen::Field::text))
                .unwrap_or_default()
        });
        let line = value.trim().to_string();
        message.clear();
        typed.clear();
        let mode = match a {
            aid::ENTER => RunMode::Run,
            a => match aid::function_of(a) {
                Some(3 | 12) => return Act::Back,
                Some(4) => RunMode::PromptRun,
                Some(9) => {
                    back = (back + 1).min(previous.len());
                    typed = previous
                        .len()
                        .checked_sub(back)
                        .and_then(|i| previous.get(i).cloned())
                        .unwrap_or_default();
                    continue;
                }
                _ => {
                    message = "Function key not allowed.".into();
                    typed = line;
                    continue;
                }
            },
        };
        if line.is_empty() {
            if mode == RunMode::PromptRun {
                message = "Type a command to prompt.".into();
            }
            continue;
        }
        back = 0;
        previous.push(line.clone());
        let o = keys::command(host, &line, mode);
        if keys::unwinding() {
            return Act::Gone;
        }
        message = keys::message_of(&o).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets() {
        assert_eq!(target("*ASSIST"), Target::Assist);
        assert_eq!(target("*NONE"), Target::Nothing);
        assert_eq!(target("QSYS/QUSCMDLN"), Target::CmdLine);
        assert_eq!(target("QUSCMDLN  QSYS      "), Target::CmdLine);
        assert_eq!(target("MYLIB/MYATN"), Target::Call("MYLIB/MYATN".into()));
        assert_eq!(target("MYATN     MYLIB     "), Target::Call("MYLIB/MYATN".into()));
    }

    #[test]
    fn window_has_its_line() {
        let mut m = tn5250::screen::ScreenModel::new(false);
        m.field(5, 10, 5, attr::GREEN_UL, "", Ffw::default(), &[]);
        let f = draw_window(&mut m, "wrkactjob", "").unwrap();
        assert_eq!(m.fields()[f].text().trim(), "wrkactjob");
        assert!(m.fields()[0].ffw.bypass);
        let rows = m.text_rows();
        assert!(rows[WIN_TOP + 1].contains("===>"));
        assert!(rows[WIN_TOP + 3].contains("F12=Cancel"));
    }
}
