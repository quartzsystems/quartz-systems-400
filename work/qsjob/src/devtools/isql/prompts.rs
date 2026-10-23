// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! F4: Select SQL Statement on a blank line, else the prompt of the
//! statement typed (and the display after it that some statements
//! need); the statement built goes on the input lines to be run.

use isql::assemble::{self, Kind, LIST, Values};
use isql::screen::menus::{self, STATEMENTS, statement_pages};
use isql::screen::prompt::{self, Prompt};

use super::Session;
use super::ui::{Key, show};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// How a prompt display was left.
enum Left {
    /// Enter, with the values.
    Done(Values),
    /// F12 or F3.
    Cancelled,
}

/// F4 on Enter SQL Statements.
pub fn prompt(host: &Host, surface: &mut Surface, s: &mut Session) {
    let _ = host;
    let text = s.h.text();
    let kind = if text.trim().is_empty() {
        match pick_statement(surface, s) {
            Some(k) => k,
            None => return,
        }
    } else if let Some(k) = assemble::kind_of(&text) {
        k
    } else {
        s.h.message("", "That statement has no prompt; type it and press Enter.");
        s.h.fit();
        return;
    };
    let typed = s.h.typed();
    let start = assemble::fields_of(kind, &text);
    if let Some(stmt) = statement(surface, s, kind, start) {
        s.h.clear_input();
        s.h.set_text(&stmt);
        s.h.fit();
        return;
    }
    let shown = if typed.is_empty() {
        vec![
            LIST.iter()
                .find(|(k, _)| *k == kind)
                .map_or(String::new(), |(_, w)| (*w).to_string()),
        ]
    } else {
        typed
    };
    s.h.statement(shown);
    s.h.message("", "The prompt was left; nothing ran.");
    s.h.clear_input();
    s.h.to_end();
}

/// A prompt display's check of its values before it is left.
type Check = Box<dyn Fn(&Values) -> Result<(), String>>;

/// Select SQL Statement: the statement chosen, when it has a prompt;
/// another one's first words go on the input instead.
fn pick_statement(surface: &mut Surface, s: &mut Session) -> Option<Kind> {
    let mut page = 0;
    let mut msg: Option<String> = None;
    loop {
        let screen = menus::statement_list(page, "", msg.take().as_deref());
        let a = show(surface, &screen)?;
        match a.key {
            Key::F(3 | 12) => return None,
            Key::PageDown if page + 1 < statement_pages() => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::PageDown | Key::PageUp => {}
            Key::Enter => {
                let sel = a.get("SELECTION");
                if sel.is_empty() {
                    continue;
                }
                let Some(word) = sel
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|i| STATEMENTS.get(i))
                else {
                    msg = Some(format!("Selection {sel} is not valid."));
                    continue;
                };
                if let Some(k) = assemble::kind_of(word) {
                    return Some(k);
                }
                s.h.clear_input();
                s.h.set_text(&format!("{word} "));
                return None;
            }
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// One prompt display until Enter (values checked by `check`) or F12.
fn display(
    surface: &mut Surface,
    p: &Prompt,
    values: &mut Values,
    sql: bool,
    check: &dyn Fn(&Values) -> Result<(), String>,
) -> Option<Left> {
    let names = prompt::names(p);
    let mut msg: Option<String> = None;
    loop {
        let screen = prompt::render(p, values, sql, msg.take().as_deref());
        let a = show(surface, &screen)?;
        for n in &names {
            if let Some(v) = a.vars.get(n) {
                values.insert(n.clone(), v.trim_end().to_string());
            }
        }
        match a.key {
            Key::F(3 | 12) => return Some(Left::Cancelled),
            Key::F(5) => {
                for n in &names {
                    values.remove(n);
                }
            }
            Key::F(4) => msg = Some("Type the name; this release has no list here.".into()),
            Key::Enter => {
                let all = prompt::gather(p, values);
                match check(&all) {
                    Ok(()) => return Some(Left::Done(all)),
                    Err(e) => msg = Some(e),
                }
            }
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

fn need(v: &Values, names: &[(&str, &str)]) -> Result<(), String> {
    for (n, what) in names {
        if v.get(*n).is_none_or(|x| x.trim().is_empty()) {
            return Err(format!("A value is needed for {what}."));
        }
    }
    Ok(())
}

/// A statement's prompts; the statement built, or `None` when left.
fn statement(surface: &mut Surface, s: &Session, kind: Kind, start: Values) -> Option<String> {
    let sql = s.sql();
    let mut v = start;
    let first = prompt::of(kind);
    let check: Check = match kind {
        Kind::Select => Box::new(|v: &Values| need(v, &[("FROM", "FROM")])),
        Kind::Insert => Box::new(|v: &Values| need(v, &[("INTO", "INTO")])),
        Kind::Update | Kind::Delete | Kind::CreateTable => {
            Box::new(|v: &Values| need(v, &[("TABLE", "the name")]))
        }
        Kind::CreateView => Box::new(|v: &Values| need(v, &[("VIEW", "the view")])),
        Kind::CreateIndex => {
            Box::new(|v: &Values| need(v, &[("INDEX", "the index"), ("TABLE", "the file")]))
        }
    };
    let Left::Done(mut v1) = display(surface, first, &mut v, sql, &*check)? else {
        return None;
    };
    // The display after the first, for the statements that need one.
    match kind {
        Kind::Insert => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("VALUES", "VALUES")]);
            let Left::Done(more) = display(surface, &prompt::INSERT_VALUES, &mut w, sql, &ok)?
            else {
                return None;
            };
            v1.extend(more);
        }
        Kind::Update => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("SET", "SET")]);
            let Left::Done(more) = display(surface, &prompt::UPDATE_SET, &mut w, sql, &ok)? else {
                return None;
            };
            v1.extend(more);
        }
        Kind::CreateIndex => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("COLUMNS", "the key")]);
            let Left::Done(more) = display(surface, &prompt::INDEX_KEY, &mut w, sql, &ok)? else {
                return None;
            };
            v1.extend(more);
        }
        Kind::CreateView => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("FROM", "FROM")]);
            let Left::Done(sel) = display(surface, &prompt::SELECT, &mut w, sql, &ok)? else {
                return None;
            };
            let sub = assemble::build(Kind::Select, &sel, sql).ok()?;
            v1.insert("SUBSELECT".into(), sub);
        }
        Kind::Select | Kind::Delete | Kind::CreateTable => {}
    }
    assemble::build(kind, &v1, sql).ok()
}
