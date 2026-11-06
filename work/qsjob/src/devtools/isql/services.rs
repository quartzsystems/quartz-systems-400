// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! F13's Interactive SQL Session Services (the attributes, printing,
//! clearing and saving the session in a source file), the Exit
//! Interactive SQL display, and a plain help display. The displays and
//! what each option leaves are pub400's
//! (`tools/conformance/golden/sql/strsql/`).

use isql::attrs::Attrs;
use isql::history::Line;
use isql::print::{self, Stamp};
use isql::saved::source_lines;
use isql::screen::menus::{self, attr_keys, attr_pages};
use isql::screen::services::{self as panels, SourceFile};
use isql::screen::{Screen, blank, keys, more, put};
use stg::Client;
use uim::render::Attr;

use super::ui::{Key, show};
use super::{Session, note, note_error, start_sql, words};
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
                "2" => match print_session(host, surface, s) {
                    Some(Ok(())) => msg = Some(crate::display::msgline::say(host, "SQL6085", &[])),
                    Some(Err(e)) => msg = Some(crate::display::msgline::of_error(host, &e)),
                    None => {}
                },
                "3" => {
                    if confirm_remove(host, surface) {
                        s.h.clear();
                        msg = Some(crate::display::msgline::say(host, "SQL6086", &[]));
                    }
                }
                "4" => {
                    if save_source(host, surface, s).is_some() {
                        return;
                    }
                }
                other => msg = Some(crate::display::msgline::say(host, "CPD6A64", &[&other])),
            },
            _ => msg = Some(crate::display::msgline::say(host, "CPD6A60", &[])),
        }
    }
}

/// The session as printed lines: statements and messages as Enter SQL
/// Statements shows them, at the left margin.
fn session_lines(s: &Session) -> Vec<String> {
    s.h.lines()
        .into_iter()
        .map(|l| match l {
            Line::Statement { text, .. } | Line::Message { text, .. } => text,
        })
        .collect()
}

/// The job's date and time now, as the job writes them.
#[allow(clippy::many_single_char_names)] // year, month, day
pub fn stamp(host: &Host) -> Stamp {
    let f = Attrs::default().format(&super::job_formats(host));
    let env = crate::display::dds::env_of(host, "");
    let (y, m, d) = env.date;
    let (h, mi, sec) = env.time;
    Stamp {
        date: f.date(&format!("{y:04}-{m:02}-{d:02}")),
        time: f.time(&format!("{h:02}.{mi:02}.{sec:02}")),
    }
}

/// Change Printer File, then the session printed; `None` when left
/// with F3 or F12.
fn print_session(host: &Host, surface: &mut Surface, s: &Session) -> Option<Result<(), String>> {
    let w = words(host);
    let mut file = "QSYSPRT".to_string();
    let mut msg: Option<String> = None;
    loop {
        let a = show(surface, &panels::printer_file(&file, msg.take().as_deref(), &w))?;
        file = a.up("PRTF");
        match a.key {
            Key::F(3 | 12) => return None,
            Key::F(5) => file = "QSYSPRT".into(),
            Key::Enter if file.is_empty() => msg = Some(crate::display::msgline::say(host, "CPD9BF4", &[])),
            Key::Enter => {
                let pages = print::session(
                    &session_lines(s),
                    "QS00ST1",
                    &release::RELEASE.to_string(),
                    &stamp(host),
                    60,
                    &w,
                );
                return Some(super::run::spool(host, &file, &pages));
            }
            _ => msg = Some(crate::display::msgline::say(host, "CPD6A60", &[])),
        }
    }
}

/// Confirm Remove Entries Request; true on Enter.
fn confirm_remove(host: &Host, surface: &mut Surface) -> bool {
    let w = words(host);
    show(surface, &panels::confirm_remove(&w)).is_some_and(|a| a.key == Key::Enter)
}

/// Change Session Attributes; true when F3 asked to leave the services.
/// Enter applies the values unless they change what the display shows
/// (SELECT output's rows, the separators a format takes): then it is
/// shown again with them, as pub400 does.
fn attributes(host: &Host, surface: &mut Surface, s: &mut Session) -> bool {
    let w = words(host);
    let mut work = s.attrs.clone();
    let mut page = 0;
    let mut msg: Option<String> = None;
    loop {
        let screen = menus::attributes(page, &work, msg.take().as_deref(), &w);
        let Some(a) = show(surface, &screen) else {
            return true;
        };
        let before = layout(&work, &w);
        // What was typed on this page, checked as it is read.
        for k in attr_keys(page, &work, &w) {
            if let Some(v) = a.vars.get(k)
                && let Err(e) = work.set(k, v)
            {
                msg = Some(crate::display::msgline::text_of(host, &e.msg()));
            }
        }
        match a.key {
            Key::F(3) => return true,
            Key::F(12) => return false,
            Key::F(5) => work = s.attrs.clone(),
            Key::F(4) => msg = Some(crate::display::msgline::say(host, "CPI9BD9", &[])),
            Key::PageDown if page + 1 < attr_pages(&work, &w) => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::Enter if msg.is_none() && layout(&work, &w) == before => {
                apply(host, s, work);
                return false;
            }
            _ => page = page.min(attr_pages(&work, &w) - 1),
        }
    }
}

/// The keys every page shows, to see whether the display changed.
fn layout(a: &Attrs, w: &dyn Fn(&str) -> String) -> Vec<Vec<&'static str>> {
    (0..attr_pages(a, w)).map(|p| attr_keys(p, a, w)).collect()
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
            note(host, &mut s.h, "CPI9BF0", &[&n]);
        }
    }
    if (commit_changed || naming_changed)
        && let Err(e) = start_sql(host, s)
    {
        note_error(host, &mut s.h, &e);
    }
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
                    // Bound first: the link's borrow ends before the message is read.
                    let put = host.link.borrow_mut().with(|l| l.isql_session_put(Some(text)));
                    if let Err(e) = put {
                        msg = Some(crate::display::msgline::text_of(host, &crate::panels::line(&e)));
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
                _ => msg = Some(crate::display::msgline::say(host, "CPD9B98", &[])),
            },
            _ => msg = Some(crate::display::msgline::say(host, "CPD6A60", &[])),
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

/// Change Source File: the statements written to a member (options 1
/// to 4 create or replace it, 5 adds to it); what happened goes into
/// the history; `None` when cancelled. A new file's authority and text
/// other than the defaults are refused (SQL0084), not ignored.
fn save_source(host: &Host, surface: &mut Surface, s: &mut Session) -> Option<()> {
    let txt = words(host);
    let lib = host.attrs().libl.cur.unwrap_or_else(|| "QGPL".into());
    let mut form = SourceFile::new(&lib);
    let mut msg: Option<String> = None;
    loop {
        let a = show(surface, &panels::source_file(&form, msg.take().as_deref(), &txt))?;
        form.file = a.up("FILE");
        form.lib = a.up("LIB");
        form.member = a.up("MBR");
        form.option = a.get("OPTION");
        form.aut = a.up("AUT");
        form.text = a.get("TEXT");
        match a.key {
            Key::F(3 | 12) => return None,
            Key::F(5) => form = SourceFile::new(&lib),
            Key::Enter => {
                if form.file.is_empty() || form.lib.is_empty() || form.member.is_empty() {
                    msg = Some(crate::display::msgline::say(host, "CPD9BF4", &[]));
                    continue;
                }
                if !matches!(form.option.as_str(), "1" | "2" | "3" | "4" | "5") {
                    msg = Some(crate::display::msgline::say(host, "CPD9B98", &[]));
                    continue;
                }
                if form.aut != "*EXCLUDE" || !form.text.is_empty() {
                    msg = Some(crate::display::msgline::say(host, "SQL0084", &[&"AUT TEXT"]));
                    continue;
                }
                // `*FILE` and `*FIRST` name the member after the file.
                let member = if form.member.starts_with('*') { form.file.clone() } else { form.member.clone() };
                let mut lines = source_lines(&s.h.entries);
                if form.option == "5"
                    && let Ok(Some(mut old)) =
                        host.link.borrow_mut().with(|l| l.pdm_read(form.lib.clone(), form.file.clone(), member.clone()))
                {
                    old.append(&mut lines);
                    lines = old;
                }
                let r = host
                    .link
                    .borrow_mut()
                    .with(|l| l.pdm_write(form.lib.clone(), form.file.clone(), member.clone(), lines, "SQL".into()));
                match r {
                    Ok(()) => note(host, &mut s.h, "CPI9BEF", &[&member, &form.lib, &form.file]),
                    Err(e) => note_error(host, &mut s.h, &e),
                }
                return Some(());
            }
            _ => msg = Some(crate::display::msgline::say(host, "CPD6A60", &[])),
        }
    }
}

/// A help display: the title centred, the lines from row 3, paged.
pub fn help_display(surface: &mut Surface, heading: &str, lines: &[String]) {
    const PER: usize = 17;
    let mut page = 0;
    loop {
        let mut s: Screen = blank();
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
