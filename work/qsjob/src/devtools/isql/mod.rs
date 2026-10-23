// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Interactive SQL in the job: `STRSQL`'s Enter SQL Statements and
//! everything it leads to. The session, its history, the prompts and
//! the displays' layouts are the `isql` crate's; the statements run in
//! the storage service in the job's own SQL context (requests 400-404),
//! which also keeps each user's saved session in `QUSRSYS/QSQLSESS`.

mod lists;
mod prompts;
mod run;
mod services;
mod ui;

use cmd::{Resolved, Value};
use isql::attrs::{Attrs, lock_level};
use isql::history::{AREA, History, Line, WIDTH};
use isql::saved;
use isql::screen::enter::{self, AREA_ROW, INPUT_COL, Slot, line_name};
use stg::Client;

use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::panels::shows_panels;
use ui::{Answer, Key, show};

/// The most rows a query brings back for Display Data.
const MAX_ROWS: u32 = 10_000;

/// A STRSQL session.
pub struct Session {
    /// The attributes.
    pub attrs: Attrs,
    /// The history and the input.
    pub h: History,
    /// The key page (F24).
    page: u8,
    /// The libraries and files chosen on the select lists, for F17 and
    /// F18.
    pub libs: Vec<String>,
    /// The files chosen, `(library, file)`.
    pub files: Vec<(String, String)>,
    /// The session started commitment control, so ends it.
    pub own_cmt: bool,
}

impl Session {
    /// `*SQL` naming.
    fn sql(&self) -> bool {
        self.attrs.sql_naming()
    }

    /// The naming for the service.
    fn naming(&self) -> String {
        self.attrs.get("NAMING")
    }

    /// The saved form.
    fn saved(&self) -> String {
        saved::to_text(&saved::Saved {
            attrs: self.attrs.clone(),
            entries: self.h.entries.clone(),
            input: self.h.input.clone(),
        })
    }
}

/// A parameter's value as typed, upper case; `None` when not given.
fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::List(items) => items.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        })
        .map(|s| s.trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|s| !s.is_empty())
}

/// Start the job's SQL session as the attributes say.
fn start_sql(host: &Host, s: &mut Session) -> Result<(), String> {
    let level = lock_level(&s.attrs.get("COMMIT")).unwrap_or("*NONE");
    let started = host
        .link
        .borrow_mut()
        .with(|l| l.isql_start(s.attrs.get("NAMING"), level.to_string()))?;
    s.own_cmt |= started;
    Ok(())
}

/// `STRSQL`: Enter SQL Statements until the session ends.
pub fn run(host: &Host, r: &Resolved) -> Result<String, Fail> {
    if !shows_panels(host, r) {
        return Err(Fail::new(
            "CPF9898",
            "STRSQL needs a display; use RUNSQLSTM in a batch job.",
        ));
    }
    let mut surface = Surface::new(host.clone());
    let mut s = open(host, r);
    if let Err(e) = start_sql(host, &mut s) {
        s.h.message("", &format!("Commitment control did not start: {e}"));
    }
    let rdb = crate::display::dds::system_name(host);
    s.h.message("", &format!("Connected to relational database {rdb}."));
    s.h.fit();
    main_loop(host, &mut surface, &mut s);
    Ok(String::new())
}

/// The session: the saved one when the user has one, else a new one
/// with the command's attributes.
fn open(host: &Host, r: &Resolved) -> Session {
    let stored = host
        .link
        .borrow_mut()
        .with(Client::isql_session)
        .and_then(|t| saved::from_text(&t).ok());
    let mut s = Session {
        attrs: Attrs::default(),
        h: History::new(),
        page: 0,
        libs: Vec::new(),
        files: Vec::new(),
        own_cmt: false,
    };
    if let Some(saved) = stored {
        s.attrs = saved.attrs;
        s.h.entries = saved.entries;
        s.h.input = saved.input;
        s.h.message("", "The saved session was restored.");
        s.h.to_end();
        return s;
    }
    for f in isql::attrs::FIELDS {
        if let Some(v) = parm(r, f.key) {
            let _ = s.attrs.set(f.key, &v);
        }
    }
    s
}

/// Copy the input lines off the display into the history.
fn take_input(s: &mut Session, a: &Answer) {
    for (i, line) in s.h.input.iter_mut().enumerate() {
        if let Some(v) = a.vars.get(&line_name(i)) {
            *line = v
                .chars()
                .take(WIDTH)
                .collect::<String>()
                .trim_end()
                .to_string();
        }
    }
}

/// What the cursor is on in the area.
fn slot_at(s: &Session, cursor: (usize, usize)) -> Option<Slot> {
    let (row, _) = cursor;
    if !(AREA_ROW..AREA_ROW + AREA).contains(&row) {
        return None;
    }
    enter::slots(&s.h).get(row - AREA_ROW).copied()
}

/// The input line the cursor is on, and its column in the line.
fn input_at(s: &Session, cursor: (usize, usize)) -> Option<(usize, usize)> {
    match slot_at(s, cursor) {
        Some(Slot::Input(i)) => Some((i, cursor.1.saturating_sub(INPUT_COL))),
        _ => None,
    }
}

const OUT_OF_RANGE: &str = "Place the cursor on an input line for this key.";

/// Enter SQL Statements until the session ends.
fn main_loop(host: &Host, surface: &mut Surface, s: &mut Session) {
    let mut msg: Option<String> = None;
    let mut cursor: Option<(usize, usize)> = None;
    loop {
        let screen = enter::enter(&s.h, s.page, s.sql(), msg.take().as_deref(), cursor.take());
        let Some(a) = show(surface, &screen) else {
            return;
        };
        take_input(s, &a);
        match a.key {
            Key::Enter => {
                if s.h.text().is_empty() {
                    continue;
                }
                run::statement(host, surface, s);
            }
            Key::F(3 | 12) => {
                if services::exit(host, surface, s) {
                    return;
                }
            }
            Key::F(4) => prompts::prompt(host, surface, s),
            Key::F(6) => match input_at(s, a.cursor) {
                Some((i, _)) => {
                    s.h.insert_line(i);
                    s.h.show_input();
                    cursor = row_of(s, i + 1);
                }
                None => msg = Some(OUT_OF_RANGE.into()),
            },
            Key::F(10) => match input_at(s, a.cursor) {
                Some((i, _)) => {
                    s.h.copy_line(i);
                    s.h.show_input();
                    cursor = row_of(s, i + 1);
                }
                None => msg = Some(OUT_OF_RANGE.into()),
            },
            Key::F(14) => match input_at(s, a.cursor) {
                Some((i, _)) => s.h.delete_line(i),
                None => msg = Some(OUT_OF_RANGE.into()),
            },
            Key::F(15) => match input_at(s, a.cursor) {
                Some((i, col)) => {
                    s.h.split_line(i, col);
                    s.h.show_input();
                    cursor = row_of(s, i + 1);
                }
                None => msg = Some(OUT_OF_RANGE.into()),
            },
            Key::F(9) => msg = retrieve(s, a.cursor),
            Key::F(13) => services::services(host, surface, s),
            Key::F(n @ 16..=18) => match input_at(s, a.cursor) {
                Some(at) => msg = lists::select(host, surface, s, n, at),
                None => msg = Some(OUT_OF_RANGE.into()),
            },
            Key::F(24) => s.page = (s.page + 1) % 2,
            Key::PageUp => {
                if s.h.top == 0 {
                    msg = Some("This is the start of the session.".into());
                }
                s.h.page_up();
            }
            Key::PageDown => {
                if s.h.at_end() {
                    msg = Some("This is the end of the session.".into());
                }
                s.h.page_down();
            }
            Key::Help => run::help(host, surface, s, a.cursor),
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// The screen position of input line `i`, for the cursor.
fn row_of(s: &Session, i: usize) -> Option<(usize, usize)> {
    enter::slots(&s.h)
        .iter()
        .position(|x| *x == Slot::Input(i))
        .map(|k| (AREA_ROW + k, INPUT_COL))
}

/// F9: the statement under the cursor, else the next earlier one.
fn retrieve(s: &mut Session, cursor: (usize, usize)) -> Option<String> {
    if let Some(Slot::History(k)) = slot_at(s, cursor) {
        let lines = s.h.lines();
        let index = lines[..=k].iter().rev().find_map(|l| match l {
            Line::Statement { index, .. } => Some(*index),
            Line::Message { .. } => None,
        });
        if let Some(st) = index.and_then(|i| s.h.statement_at(i).cloned()) {
            s.h.clear_input();
            s.h.set_lines(&st);
            s.h.fit();
            return None;
        }
    }
    if s.h.retrieve() {
        s.h.fit();
        None
    } else {
        Some("There is no statement to retrieve.".into())
    }
}

/// Text put into the input where the cursor is, the lines run
/// together and cut again.
pub fn insert_at(s: &mut Session, (line, col): (usize, usize), text: &str) {
    let mut flat: Vec<char> =
        s.h.input
            .iter()
            .flat_map(|l| {
                let mut l: String = l.chars().take(WIDTH).collect();
                while l.chars().count() < WIDTH {
                    l.push(' ');
                }
                l.chars().collect::<Vec<char>>()
            })
            .collect();
    let at = (line * WIDTH + col.min(WIDTH)).min(flat.len());
    let ins: Vec<char> = text.chars().collect();
    flat.splice(at..at, ins);
    let joined: String = flat.into_iter().collect();
    s.h.set_text(joined.trim_end());
    s.h.show_input();
}
