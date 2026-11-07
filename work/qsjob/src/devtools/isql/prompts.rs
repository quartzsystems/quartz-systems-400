// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! F4: Select SQL Statement on a blank line, else the prompt of the
//! statement typed (and the display after it that some statements
//! need); the statement built goes on the input lines to be run.

use isql::IsqlError;
use isql::assemble::{self, Kind, Values};
use isql::screen::columns::{self, List};
use isql::screen::menus::{self, STATEMENTS, statement_pages};
use isql::screen::prompt::{self, Prompt};
use isql::screen::stmt::{self, Panel};
use stg::Client;

use super::Session;
use super::ui::{Answer, Key, show};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// How a prompt display was left.
enum Left {
    /// Enter, with the values.
    Done(Values),
    /// F12 or F3.
    Cancelled,
}

/// F4 on Enter SQL Statements. A prompt chosen on Select SQL
/// Statement and left with F12 goes back to the list; one reached from
/// the statement typed leaves the statement in the history with
/// SQL6130, as pub400 does.
pub fn prompt(host: &Host, surface: &mut Surface, s: &mut Session) {
    let text = s.h.text();
    let from_list = text.trim().is_empty();
    loop {
        let kind = if from_list {
            match pick_statement(surface, s) {
                Some(k) => k,
                None => return,
            }
        } else if let Some(k) = assemble::kind_of(&text) {
            k
        } else {
            unprompted(host, s, &text);
            return;
        };
        let start = if from_list { Values::new() } else { assemble::fields_of(kind, &text) };
        if let Some(stmt) = statement(host, surface, s, kind, start) {
            s.h.clear_input();
            s.h.set_text(&stmt);
            s.h.fit();
            return;
        }
        if from_list {
            continue;
        }
        s.h.statement(s.h.typed());
        super::note(host, &mut s.h, "SQL6130", &[]);
        s.h.clear_input();
        s.h.to_end();
        return;
    }
}

/// F4 on a statement without a prompt of its own. GRANT and REVOKE are
/// prompted from the list only; typed, they are checked, so a partial
/// one is the syntax error pub400 shows (`grant package`: SQL0104).
fn unprompted(host: &Host, s: &mut Session, text: &str) {
    let first = text.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
    if matches!(first.as_str(), "GRANT" | "REVOKE") {
        let r = host.link.borrow_mut().with(|l| l.isql_run(text.to_string(), s.naming(), true, 0));
        if r.sqlcode < 0 {
            let lines = s.h.typed();
            s.h.statement(lines.clone());
            if r.text.is_empty() {
                super::note(host, &mut s.h, &r.msgid, &[]);
            } else {
                s.h.message(&r.msgid, &r.text);
            }
            s.h.input = lines;
            s.h.to_last_statement();
            return;
        }
    }
    super::note(host, &mut s.h, "CPD9CD4", &[]);
    s.h.fit();
}

/// A prompt display's check of its values before it is left.
type Check = Box<dyn Fn(&Values) -> Result<(), IsqlError>>;

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
                    msg = Some(crate::display::msgline::say(surface.host(), "CPD6A64", &[&sel]));
                    continue;
                };
                if let Some(k) = assemble::listed(word) {
                    return Some(k);
                }
                s.h.clear_input();
                s.h.set_text(&format!("{word} "));
                return None;
            }
            _ => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A60", &[])),
        }
    }
}

/// The fields `names` as typed into `values`.
fn take(a: &Answer, names: &[String], values: &mut Values) {
    for n in names {
        if let Some(v) = a.vars.get(n) {
            values.insert(n.clone(), v.trim_end().to_string());
        }
    }
}

fn error_text(surface: &Surface, e: &IsqlError) -> String {
    crate::display::msgline::text_of(surface.host(), &e.msg())
}

/// One prompt display until Enter (values checked by `check`) or F12.
fn display(
    surface: &mut Surface,
    p: &Prompt,
    values: &mut Values,
    sql: bool,
    note: Option<String>,
    check: &dyn Fn(&Values) -> Result<(), IsqlError>,
) -> Option<Left> {
    let names = prompt::names(p);
    let mut msg = note;
    loop {
        let screen = prompt::render(p, values, sql, msg.take().as_deref());
        let a = show(surface, &screen)?;
        take(&a, &names, values);
        match a.key {
            Key::F(3 | 12) => return Some(Left::Cancelled),
            Key::F(5) => {
                for n in &names {
                    values.remove(n);
                }
            }
            Key::F(4) => msg = Some(crate::display::msgline::say(surface.host(), "CPD9BF1", &[])),
            Key::Enter => {
                let all = prompt::gather(p, values);
                match check(&all) {
                    Ok(()) => return Some(Left::Done(all)),
                    Err(e) => msg = Some(error_text(surface, &e)),
                }
            }
            _ => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A60", &[])),
        }
    }
}

/// One of the other statements' prompts until Enter builds its
/// statement or F12; Page Down and Page Up turn CALL's parameter pages.
fn panel(host: &Host, surface: &mut Surface, p: &Panel, kind: Kind, start: Values, sql: bool) -> Option<String> {
    let w = super::words(host);
    let mut values = start;
    let mut page = 0;
    let mut msg: Option<String> = None;
    loop {
        let screen = stmt::render(p, &values, page, sql, msg.take().as_deref(), &w);
        let a = show(surface, &screen)?;
        take(&a, &stmt::names(p, page), &mut values);
        match a.key {
            Key::F(3 | 12) => return None,
            Key::F(5) => values.clear(),
            Key::F(4) => msg = Some(crate::display::msgline::say(surface.host(), "CPD9BF1", &[])),
            Key::PageDown if page + 1 < stmt::pages(p) => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::PageDown => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A69", &[])),
            Key::PageUp => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A66", &[])),
            Key::Enter => match assemble::build(kind, &stmt::defaults(p, &values), sql) {
                Ok(text) => return Some(text),
                Err(e) => msg = Some(error_text(surface, &e)),
            },
            _ => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A60", &[])),
        }
    }
}

fn need(v: &Values, names: &[(&str, &'static str)]) -> Result<(), IsqlError> {
    for (n, what) in names {
        if v.get(*n).is_none_or(|x| x.trim().is_empty()) {
            return Err(IsqlError::Missing(what));
        }
    }
    Ok(())
}

fn trimmed(v: &Values, k: &str) -> String {
    v.get(k).map(|s| s.trim().to_ascii_uppercase()).unwrap_or_default()
}

/// The columns of the file `name` in `lib` (the library list's first
/// with it when `lib` is blank or `*LIBL`); `None` when there is none.
fn columns_of(host: &Host, lib: &str, name: &str) -> Option<Vec<String>> {
    let lib = if lib.is_empty() || lib == "*LIBL" {
        host.attrs().libl.search().into_iter().find(|l| {
            host.link
                .borrow_mut()
                .with(|c| c.object(l.clone(), name.to_string(), "*FILE".into()))
                .is_some()
        })?
    } else {
        lib.to_string()
    };
    let cols: Vec<String> =
        super::lists::fields(host, &[(lib, name.to_string())]).into_iter().map(|e| e.name).collect();
    (!cols.is_empty()).then_some(cols)
}

/// SQL0204 for a file not found.
fn not_found(host: &Host, lib: &str, name: &str) -> String {
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    crate::display::msgline::say(host, "SQL0204", &[&name, &lib])
}

/// The first display of INSERT, UPDATE or CREATE INDEX until Enter
/// names a file that has columns: its values and the columns.
#[allow(clippy::too_many_arguments)] // the display and what it needs
fn first_with_columns(
    host: &Host,
    surface: &mut Surface,
    p: &Prompt,
    v: &mut Values,
    sql: bool,
    check: &dyn Fn(&Values) -> Result<(), IsqlError>,
    file: (&str, &str),
    columns: bool,
) -> Option<(Values, Vec<String>)> {
    let mut note = None;
    loop {
        let Left::Done(v1) = display(surface, p, v, sql, note.take(), check)? else {
            return None;
        };
        if !columns {
            return Some((v1, Vec::new()));
        }
        let (name, lib) = (trimmed(&v1, file.0), trimmed(&v1, file.1));
        match columns_of(host, &lib, &name) {
            Some(c) => return Some((v1, c)),
            None => note = Some(not_found(host, &lib, &name)),
        }
    }
}

/// A list of the columns (`list`) until Enter takes it, checked by
/// `check`, or F12; UPDATE's first display stays above its list.
#[allow(clippy::too_many_arguments)] // the display and what it needs
fn column_list(
    host: &Host,
    surface: &mut Surface,
    list: List,
    cols: &[String],
    first: &mut Values,
    sql: bool,
    check: &dyn Fn(&Values) -> Result<(), IsqlError>,
) -> Option<Values> {
    let w = super::words(host);
    let mut values = Values::new();
    let mut page = 0;
    let mut msg: Option<String> = None;
    loop {
        let m = msg.take();
        let screen = match list {
            List::Insert => columns::insert(cols, &values, page, sql, m.as_deref(), &w),
            List::Update => {
                let top = prompt::render(&prompt::UPDATE, first, sql, None);
                columns::update(top, cols, &values, page, sql, m.as_deref(), &w)
            }
            List::Key => columns::key(cols, &values, page, sql, m.as_deref(), &w),
        };
        let a = show(surface, &screen)?;
        take(&a, &columns::names(list, cols.len(), page), &mut values);
        if list == List::Update {
            take(&a, &prompt::names(&prompt::UPDATE), first);
        }
        match a.key {
            Key::F(3 | 12) => return None,
            Key::F(5) => values.clear(),
            Key::PageDown if page + 1 < list.pages(cols.len()) => page += 1,
            Key::PageUp if page > 0 => page -= 1,
            Key::PageDown => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A69", &[])),
            Key::PageUp => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A66", &[])),
            Key::Enter => match check(&values) {
                Ok(()) => return Some(values),
                Err(e) => msg = Some(error_text(surface, &e)),
            },
            // BLOCKED(rel): F6, F10, F11, F14, F15, F19, F20 and F24 of
            // these lists are not captured past their key lines.
            _ => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A60", &[])),
        }
    }
}

/// CREATE INDEX's key typed right: numbers 1 to 999, orders A or D.
fn key_typed(cols: &[String], v: &Values) -> Result<(), IsqlError> {
    for i in 0..cols.len() {
        let seq = v.get(&columns::seq_name(i)).map_or("", |s| s.trim());
        if !seq.is_empty() && !seq.parse::<u16>().is_ok_and(|n| (1..=999).contains(&n)) {
            return Err(IsqlError::BadValue { field: "SEQ", value: seq.to_string() });
        }
        let ord = v.get(&columns::order_name(i)).map_or("", |s| s.trim());
        if !matches!(ord.to_ascii_uppercase().as_str(), "" | "A" | "D") {
            return Err(IsqlError::BadValue { field: "ORDER", value: ord.to_string() });
        }
    }
    if columns::key_columns(cols, v).is_empty() {
        return Err(IsqlError::Missing("SEQ"));
    }
    Ok(())
}

/// A statement's prompts; the statement built, or `None` when left.
fn statement(host: &Host, surface: &mut Surface, s: &Session, kind: Kind, start: Values) -> Option<String> {
    let sql = s.sql();
    let Some(first) = prompt::of(kind) else {
        let p = stmt::of(kind)?;
        return panel(host, surface, p, kind, start, sql);
    };
    let mut v = start;
    let check: Check = match kind {
        Kind::Select => Box::new(|v: &Values| need(v, &[("FROM", "FROM")])),
        Kind::Insert => Box::new(|v: &Values| need(v, &[("INTO", "INTO")])),
        Kind::CreateView => Box::new(|v: &Values| need(v, &[("VIEW", "the view")])),
        Kind::CreateIndex => Box::new(|v: &Values| need(v, &[("INDEX", "the index"), ("TABLE", "the file")])),
        _ => Box::new(|v: &Values| need(v, &[("TABLE", "the name")])),
    };
    let (file, columns_next) = match kind {
        Kind::Insert => (("INTO", "LIB"), true),
        Kind::Update => (("TABLE", "LIB"), true),
        Kind::CreateIndex => (("TABLE", "TABLELIB"), true),
        _ => (("", ""), false),
    };
    let (mut v1, cols) = first_with_columns(host, surface, first, &mut v, sql, &*check, file, columns_next)?;
    // The display after the first, for the statements that need one.
    match kind {
        Kind::Insert if trimmed(&v1, "METHOD") == "2" => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("FROM", "FROM")]);
            let Left::Done(sel) = display(surface, &prompt::SELECT, &mut w, sql, None, &ok)? else {
                return None;
            };
            let sub = assemble::build(Kind::Select, &sel, sql).ok()?;
            v1.insert("VALUES".into(), sub);
            v1.insert("METHOD".into(), "2".into());
        }
        Kind::Insert => {
            let ok = |x: &Values| {
                if columns::insert_values(&cols, x).0.is_empty() { Err(IsqlError::Missing("VALUES")) } else { Ok(()) }
            };
            let more = column_list(host, surface, List::Insert, &cols, &mut v1, sql, &ok)?;
            let (names, vals) = columns::insert_values(&cols, &more);
            v1.insert("COLUMNS".into(), names.join(", "));
            v1.insert("VALUES".into(), vals.join(", "));
        }
        Kind::Update => {
            let ok = |x: &Values| {
                if columns::insert_values(&cols, x).0.is_empty() { Err(IsqlError::Missing("SET")) } else { Ok(()) }
            };
            let more = column_list(host, surface, List::Update, &cols, &mut v1, sql, &ok)?;
            let (names, vals) = columns::insert_values(&cols, &more);
            let set: Vec<String> = names.iter().zip(&vals).map(|(n, val)| format!("{n} = {val}")).collect();
            v1.insert("SET".into(), set.join(", "));
            // BLOCKED(rel): the WHERE display after this one is not
            // captured; the statement ends in WHERE for the input lines
            // to finish, so it does not run over every row as it is.
            return assemble::build(kind, &v1, sql).ok().map(|t| format!("{t} WHERE "));
        }
        Kind::CreateIndex => {
            let ok = |x: &Values| key_typed(&cols, x);
            let more = column_list(host, surface, List::Key, &cols, &mut v1, sql, &ok)?;
            v1.insert("COLUMNS".into(), columns::key_columns(&cols, &more).join(", "));
        }
        Kind::CreateView => {
            let mut w = Values::new();
            let ok = |x: &Values| need(x, &[("FROM", "FROM")]);
            let Left::Done(sel) = display(surface, &prompt::SELECT, &mut w, sql, None, &ok)? else {
                return None;
            };
            let sub = assemble::build(Kind::Select, &sel, sql).ok()?;
            v1.insert("SUBSELECT".into(), sub);
        }
        _ => {}
    }
    assemble::build(kind, &v1, sql).ok()
}
