// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! F13's Interactive SQL Session Services (the attributes, printing,
//! clearing and saving the session in a source file), the Exit
//! Interactive SQL display, and a plain help display.

use isql::attrs::Attrs;
use isql::history::Line;
use isql::saved::source_lines;
use isql::screen::menus::{self, ATTR_PAGES, attr_keys};
use isql::screen::{Screen, blank, keys, leader, message, more, put, title, var};
use stg::Client;
use uim::render::Attr;

use super::ui::{Key, show};
use super::{Session, start_sql};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The Services menu until F3 or F12.
pub fn services(host: &Host, surface: &mut Surface, s: &mut Session) {
    let mut msg: Option<String> = None;
    loop {
        let Some(a) = show(surface, &menus::services("", msg.take().as_deref())) else {
            return;
        };
        match a.key {
            Key::F(3 | 12) => return,
            Key::Enter => match a.get("SELECTION").as_str() {
                "" => {}
                "1" => {
                    if attributes(host, surface, s) {
                        return;
                    }
                }
                "2" => {
                    let lines = session_lines(s);
                    s.h.message("", &super::run::print(host, &lines));
                    return;
                }
                "3" => {
                    s.h.clear();
                    return;
                }
                "4" => {
                    if let Some(m) = save_source(host, surface, s) {
                        s.h.message("", &m);
                        return;
                    }
                }
                other => msg = Some(format!("Selection {other} is not valid.")),
            },
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// The session as printed lines.
fn session_lines(s: &Session) -> Vec<String> {
    s.h.lines()
        .into_iter()
        .map(|l| match l {
            Line::Statement { text, first, .. } => {
                format!("{} {text}", if first { ">" } else { " " })
            }
            Line::Message { text, .. } => format!("    {text}"),
        })
        .collect()
}

/// Change Session Attributes; true when F3 asked to leave the services.
fn attributes(host: &Host, surface: &mut Surface, s: &mut Session) -> bool {
    let mut work = s.attrs.clone();
    let mut page = 0;
    let mut msg: Option<String> = None;
    loop {
        let screen = menus::attributes(page, &work, msg.take().as_deref());
        let Some(a) = show(surface, &screen) else {
            return true;
        };
        // What was typed on this page, checked as it is read.
        for k in attr_keys(page) {
            if let Some(v) = a.vars.get(k)
                && let Err(e) = work.set(k, v)
            {
                msg = Some(e.to_string());
            }
        }
        match a.key {
            Key::F(3) => return true,
            Key::F(12) => return false,
            Key::F(5) => work = s.attrs.clone(),
            Key::F(4) => msg = Some("The choices are listed beside each field.".into()),
            Key::PageDown if page + 1 < ATTR_PAGES => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::Enter if msg.is_none() => {
                apply(host, s, work);
                return false;
            }
            _ => {}
        }
    }
}

/// New attributes in force: the service told of a naming or
/// commitment control change.
fn apply(host: &Host, s: &mut Session, new: Attrs) {
    let commit_changed = new.get("COMMIT") != s.attrs.get("COMMIT");
    let naming_changed = new.get("NAMING") != s.attrs.get("NAMING");
    s.attrs = new;
    if commit_changed && s.own_cmt {
        s.own_cmt = false;
        let ended = host.link.borrow_mut().with(|l| l.isql_end(true));
        if let Ok(n) = ended
            && n > 0
        {
            s.h.message("", &format!("{n} pending changes were committed."));
        }
    }
    if (commit_changed || naming_changed)
        && let Err(e) = start_sql(host, s)
    {
        s.h.message("", &format!("The new attributes did not take: {e}"));
    }
    s.h.message("", "Session attributes changed.");
}

/// Exit Interactive SQL; true when the session ends.
pub fn exit(host: &Host, surface: &mut Surface, s: &mut Session) -> bool {
    let mut option = "1".to_string();
    let mut msg: Option<String> = None;
    loop {
        let Some(a) = show(surface, &menus::exit(&option, msg.take().as_deref())) else {
            end(host, s, true);
            return true;
        };
        option = a.get("OPTION");
        match a.key {
            Key::F(12) => return false,
            Key::Enter => match option.as_str() {
                "1" => {
                    let text = s.saved();
                    if let Err(e) = host
                        .link
                        .borrow_mut()
                        .with(|l| l.isql_session_put(Some(text)))
                    {
                        msg = Some(format!("The session was not saved: {e}"));
                        continue;
                    }
                    end(host, s, true);
                    return true;
                }
                "2" => {
                    let _ = host.link.borrow_mut().with(|l| l.isql_session_put(None));
                    end(host, s, true);
                    return true;
                }
                "3" => return false,
                "4" => {
                    if save_source(host, surface, s).is_some() {
                        let _ = host.link.borrow_mut().with(|l| l.isql_session_put(None));
                        end(host, s, true);
                        return true;
                    }
                }
                _ => msg = Some("Type 1, 2, 3 or 4.".into()),
            },
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// End the job's SQL session: the commitment control it started
/// ended, pending changes committed or rolled back.
fn end(host: &Host, s: &Session, commit: bool) {
    if s.own_cmt {
        let _ = host.link.borrow_mut().with(|l| l.isql_end(commit));
    }
}

/// Save Session in Source File: the statements written to a member;
/// the message, or `None` when cancelled.
fn save_source(host: &Host, surface: &mut Surface, s: &Session) -> Option<String> {
    let mut file = "QSQLSRC".to_string();
    let mut lib = host.attrs().libl.cur.unwrap_or_else(|| "QGPL".into());
    let mut member = "QSQLSESS".to_string();
    let mut msg: Option<String> = None;
    loop {
        let screen = save_screen(&file, &lib, &member, msg.take().as_deref());
        let a = show(surface, &screen)?;
        file = a.up("FILE");
        lib = a.up("LIB");
        member = a.up("MBR");
        match a.key {
            Key::F(3 | 12) => return None,
            Key::Enter => {
                if file.is_empty() || lib.is_empty() || member.is_empty() {
                    msg = Some("Type a file, a library and a member.".into());
                    continue;
                }
                let lines = source_lines(&s.h.entries);
                let r = host.link.borrow_mut().with(|l| {
                    l.pdm_write(
                        lib.clone(),
                        file.clone(),
                        member.clone(),
                        lines,
                        "SQL".into(),
                    )
                });
                return Some(match r {
                    Ok(()) => {
                        format!("The session's statements were saved in {lib}/{file} {member}.")
                    }
                    Err(e) => format!("The session was not saved in a source file: {e}"),
                });
            }
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// The source file, library and member to save the session in.
fn save_screen(file: &str, lib: &str, member: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 25, "Save Session in Source File");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    for (row, label, name, value, hint) in [
        (5, "File", "FILE", file, "Name"),
        (6, "Library", "LIB", lib, "Name"),
        (7, "Member", "MBR", member, "Name"),
    ] {
        put(&mut s, row, 4, Attr::Normal, &leader(label, 4, 33));
        var(&mut s, row, 37, 10, name, value);
        put(&mut s, row, 52, Attr::Normal, hint);
    }
    put(
        &mut s,
        9,
        2,
        Attr::Normal,
        "The member is made, or replaced, with the session's statements.",
    );
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((5, 37));
    s
}

/// A help display: the title centred, the lines from row 3, paged.
pub fn help_display(surface: &mut Surface, heading: &str, lines: &[String]) {
    const PER: usize = 17;
    let mut page = 0;
    loop {
        let mut s = blank();
        put(&mut s, 1, 2, Attr::Blue, "Help");
        let col = 1 + 80usize.saturating_sub(heading.len()) / 2;
        put(&mut s, 1, col, Attr::HighIntensity, heading);
        let start = page * PER;
        for (i, l) in lines.iter().skip(start).take(PER).enumerate() {
            put(&mut s, 3 + i, 6, Attr::Normal, l);
        }
        let last = start + PER >= lines.len();
        more(&mut s, 21, 79, last);
        keys(&mut s, &[(22, 2, "F3=Exit help"), (22, 18, "F12=Cancel")]);
        let Some(a) = show(surface, &s) else {
            return;
        };
        match a.key {
            Key::PageDown if !last => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::PageDown | Key::PageUp => {}
            _ => return,
        }
    }
}
