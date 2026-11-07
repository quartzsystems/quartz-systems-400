// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running the statement typed: the service runs it; a query's rows go
//! to Display Data or to `QSYSPRT`; what happened goes into the
//! history as a message. Also the help behind F1.

use isql::attrs::Output;
use isql::history::Line;
use isql::result::{Column, Data};
use isql::screen::data;
use isql::screen::enter::Slot;
use sql::embed::{Desc, Op, Request};
use stg::Client;
use stg::proto::IsqlReply;

use super::ui::{Key, show};
use super::{MAX_ROWS, Session, note, note_error, slot_at};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The message a SELECT that ran leaves.
const SELECT_DONE: &str = "SQL6072";

/// What a statement that ran says, into the history.
fn done(host: &Host, s: &mut Session, r: &IsqlReply) {
    let n = r.count;
    let (table, lib) = (&r.table.0, &r.table.1);
    let h = &mut s.h;
    match r.statement.as_str() {
        "UPDATE" | "DELETE" | "MERGE" if n == 0 => note(host, h, "SQL0100", &[&r.statement]),
        "INSERT" => note(host, h, "SQL7956", &[table, lib, &n]),
        "UPDATE" => note(host, h, "SQL7957", &[table, lib, &n]),
        "DELETE" => note(host, h, "SQL7955", &[table, lib, &n]),
        "SELECT" | "EXPLAIN" => note(host, h, SELECT_DONE, &[]),
        "CONNECT" | "SET CONNECTION" => {
            let rdb = crate::console::drda::current_rdb(&host.link);
            note(host, h, "SQL7971", &[&rdb]);
        }
        "COMMIT" => note(host, h, "SQL7960", &[]),
        "ROLLBACK" => note(host, h, "SQL7961", &[]),
        other => note(host, h, "SQL7977", &[&other]),
    }
}

/// Run the input as a statement.
pub fn statement(host: &Host, surface: &mut Surface, s: &mut Session) {
    let lines = s.h.typed();
    let check = s.attrs.syntax_only();
    let (text, r) = match decimal_point(host, s, &s.h.text()) {
        Err(r) => (String::new(), *r),
        Ok(text) if !check && let Some(r) = connection(host, s, &text) => (text, r),
        Ok(text) if !check && is_call(&text) => {
            let r = call(host, s, &text);
            (text, r)
        }
        Ok(text) => {
            let r = host
                .link
                .borrow_mut()
                .with(|l| l.isql_run(text.clone(), s.naming(), check, MAX_ROWS));
            (text, r)
        }
    };
    s.h.statement(lines.clone());
    if r.sqlcode < 0 {
        if r.text.is_empty() {
            note(host, &mut s.h, &r.msgid, &[]);
        } else {
            s.h.message(&r.msgid, &r.text);
        }
        s.h.input = lines;
        s.h.to_last_statement();
        return;
    }
    if check {
        let id = if s.attrs.get("PROCESS") == "*VLD" {
            "CPI9BEB"
        } else {
            "CPI9BEC"
        };
        note(host, &mut s.h, id, &[]);
    } else if r.columns.is_empty() {
        done(host, s, &r);
    } else {
        let d = Data {
            columns: r
                .columns
                .iter()
                .map(|(name, ty)| Column {
                    name: name.clone(),
                    data_type: ty.clone(),
                })
                .collect(),
            rows: r.rows.clone(),
            fmt: s.attrs.format(&super::job_formats(host)),
        };
        let cut = r
            .truncated
            .then(|| crate::display::msgline::say(host, "CPI9BED", &[&MAX_ROWS]));
        match s.attrs.output() {
            Output::Display => display(surface, &d, cut.as_deref()),
            Output::Printer => {
                let w = super::words(host);
                let pages = isql::print::select(&d, &super::services::stamp(host), 132, 60, &w);
                // The query report's printer file; QSYSPRT where it is
                // not there.
                let r =
                    spool(host, "QPQUPRFIL", &pages).or_else(|_| spool(host, "QSYSPRT", &pages));
                if let Err(e) = r {
                    note_error(host, &mut s.h, &e);
                }
            }
            Output::File => {
                if let Err(e) = to_file(host, s, &text) {
                    s.h.message(&e.0, &e.1);
                    s.h.input.clear();
                    s.h.to_end();
                    return;
                }
            }
        }
        done(host, s, &r);
    }
    s.h.clear_input();
    s.h.to_end();
}

/// The statement as the service reads it: under `DECPNT(*COMMA)` each
/// decimal comma a period (`sql::lex::decimal_comma`); the SQL0104 of a
/// number written wrongly there.
fn decimal_point(host: &Host, s: &Session, text: &str) -> Result<String, Box<IsqlReply>> {
    if s.attrs.format(&super::job_formats(host)).point != ',' {
        return Ok(text.to_string());
    }
    sql::lex::decimal_comma(text).map_err(|e| {
        let ca = sql::Sqlca::from_error(&e);
        Box::new(IsqlReply {
            sqlcode: ca.sqlcode,
            sqlstate: ca.sqlstate,
            msgid: e.msgid(),
            text: ca.sqlerrmc,
            ..IsqlReply::default()
        })
    })
}

/// The statements the job's connections answer (`console::drda`):
/// `CONNECT`, `SET CONNECTION`, `RELEASE`, `DISCONNECT`, and every
/// other while a remote server is current; `None` for the local
/// server's. A query at a remote server is refused
/// ([`crate::console::drda::remote_query_refused`]).
fn connection(host: &Host, s: &Session, text: &str) -> Option<IsqlReply> {
    let words: Vec<String> = text
        .split_whitespace()
        .take(2)
        .map(str::to_ascii_uppercase)
        .collect();
    let w = |i: usize| words.get(i).map_or("", String::as_str);
    let name = match (w(0), w(1)) {
        ("CONNECT" | "DISCONNECT", _) => w(0).to_string(),
        ("RELEASE", second) if second != "SAVEPOINT" && second != "TO" => "RELEASE".to_string(),
        ("SET", "CONNECTION") => "SET CONNECTION".to_string(),
        _ if crate::console::drda::remote_current() => w(0).to_string(),
        _ => return None,
    };
    let reply = |ca: sql::Sqlca, statement: String| IsqlReply {
        sqlcode: ca.sqlcode,
        msgid: if ca.sqlcode < 0 {
            ca.msgid()
        } else {
            String::new()
        },
        sqlstate: ca.sqlstate,
        text: ca.sqlerrmc,
        statement,
        ..IsqlReply::default()
    };
    if matches!(name.as_str(), "SELECT" | "WITH" | "VALUES") {
        return Some(reply(crate::console::drda::remote_query_refused(), name));
    }
    let req = Request {
        scope: "QSQISQL:".into(),
        desc: Desc {
            op: Op::Exec,
            sql_naming: s.attrs.sql_naming(),
            text: text.to_string(),
            ..Desc::default()
        },
        inputs: Vec::new(),
    };
    let r = crate::console::drda::route(&host.link, &req)?;
    Some(reply(r.sqlca, name))
}

/// Whether a statement is a `CALL`.
fn is_call(text: &str) -> bool {
    text.split_whitespace()
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case("CALL"))
}

/// A `CALL`: run as the job's embedded SQL, the procedure called in a
/// runtime job of the session's own. Its result sets are not shown, as
/// STRSQL shows none; its changes are under the procedure's own
/// commitment control.
fn call(host: &Host, s: &mut Session, text: &str) -> IsqlReply {
    let desc = Desc {
        op: Op::ExecImm,
        sql_naming: s.attrs.sql_naming(),
        ..Desc::default()
    };
    let req = Request {
        scope: "QSQISQL:".into(),
        desc,
        inputs: vec![sql::Value::Str(text.to_string())],
    };
    let job = s
        .calls
        .get_or_insert_with(|| crate::console::build_job(host).0);
    let a = host.attrs();
    job.ident.libl = a.libl.search();
    job.ident.curlib = a.libl.cur;
    let (r, _) =
        crate::console::sqlcall::run(&host.link, &req, |p| qrpgrt::sqlcall::perform(job, p));
    let ca = r.sqlca;
    IsqlReply {
        sqlcode: ca.sqlcode,
        msgid: if ca.sqlcode < 0 {
            ca.msgid()
        } else {
            String::new()
        },
        sqlstate: ca.sqlstate,
        text: ca.sqlerrmc,
        statement: "CALL".into(),
        ..IsqlReply::default()
    }
}

/// Display Data until F3 or F12.
pub fn display(surface: &mut Surface, d: &Data, note: Option<&str>) {
    let mut p = data::pager();
    let rows = d.rows.len();
    let width = d.width();
    let mut msg = note.map(str::to_string);
    loop {
        let screen = data::display(d, &p, msg.take().as_deref());
        let Some(a) = show(surface, &screen) else {
            return;
        };
        match a.key {
            Key::F(3 | 12) => return,
            Key::PageDown => {
                if p.at_bottom(rows) {
                    msg = Some(crate::display::msgline::say(surface.host(), "CPD6A69", &[]));
                }
                p.down(rows);
            }
            Key::PageUp => {
                if p.top == 0 {
                    msg = Some(crate::display::msgline::say(surface.host(), "CPD6A66", &[]));
                }
                p.up();
            }
            Key::F(19) => {
                p.left();
                if p.shift == 1 {
                    msg = Some(crate::display::msgline::say(surface.host(), "QRY1908", &[]));
                }
            }
            Key::F(20) => {
                p.right(width);
                if p.shift + p.cols > width {
                    msg = Some(crate::display::msgline::say(surface.host(), "QRY1909", &[]));
                }
            }
            Key::F(21) => {
                msg = Some(crate::display::msgline::say(surface.host(), "CPD9B81", &[]));
            }
            Key::Enter => {
                if !p.position(&a.get("POSITION"), rows) {
                    msg = Some(crate::display::msgline::say(surface.host(), "CPD9BF2", &[]));
                }
                if !p.shift_to(&a.get("SHIFT"), width) {
                    msg = Some(crate::display::msgline::say(
                        surface.host(),
                        "CPD9BF3",
                        &[&width],
                    ));
                }
            }
            _ => msg = Some(crate::display::msgline::say(surface.host(), "CPD6A60", &[])),
        }
    }
}

/// Pages to a spooled file of printer file `file`, each page's first
/// line after a skip to line 1.
pub fn spool(host: &Host, file: &str, pages: &[Vec<String>]) -> Result<(), String> {
    use spool::printer::{Control, PageFormat, Printer};
    let (id, prtf) = host
        .link
        .borrow_mut()
        .with(|l| l.prt_open(file.into(), "*LIBL".into(), "*DFTACTGRP".into()))?;
    let mut p = Printer::new(PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: prtf.fold,
    });
    for page in pages {
        for (i, l) in page.iter().enumerate() {
            let ctl = if i == 0 {
                Control {
                    skip_before: 1,
                    ..Control::default()
                }
            } else {
                Control::line()
            };
            p.write_str(l, ctl);
        }
    }
    let bytes = p.take();
    let put = host
        .link
        .borrow_mut()
        .with(|l| l.splf_put(id, bytes, p.pages, p.records));
    let closed = host.link.borrow_mut().with(|l| l.splf_close(id, prtf.hold));
    put.and(closed)
}

/// SELECT output 3: the query's rows written to the session's output
/// file through the statements that do it: option 1 `CREATE TABLE ...
/// AS (...) WITH DATA`, 2 the file dropped first, 4 its rows deleted
/// first, 5 the rows inserted. Option 3 and a member other than the
/// first are refused (SQL0084): members are not made from here.
fn to_file(host: &Host, s: &Session, query: &str) -> Result<(), (String, String)> {
    let a = &s.attrs;
    let sep = if s.sql() { "." } else { "/" };
    let target = format!("{}{sep}{}", a.get("OUTLIB"), a.get("OUTFILE"));
    let refuse = |what: &str| {
        let id = "SQL0084".to_string();
        let text = crate::display::msgline::say(host, &id, &[&what]);
        (id, text)
    };
    if !matches!(a.get("OUTMBR").as_str(), "*FILE" | "*FIRST") {
        return Err(refuse("OUTMBR"));
    }
    if a.get("OUTAUT") != "*EXCLUDE" || !a.get("OUTTEXT").is_empty() {
        return Err(refuse("OUTAUT OUTTEXT"));
    }
    let create = format!("CREATE TABLE {target} AS ({query}) WITH DATA");
    let steps: Vec<(String, bool)> = match a.get("OUTOPT").as_str() {
        "1" => vec![(create, false)],
        "2" => vec![(format!("DROP TABLE {target}"), true), (create, false)],
        "4" => vec![
            (format!("DELETE FROM {target}"), false),
            (format!("INSERT INTO {target} {query}"), false),
        ],
        "5" => vec![(format!("INSERT INTO {target} {query}"), false)],
        _ => return Err(refuse("OUTOPT(3)")),
    };
    for (stmt, may_fail) in steps {
        let r = host
            .link
            .borrow_mut()
            .with(|l| l.isql_run(stmt, s.naming(), false, 0));
        if r.sqlcode < 0 && !may_fail {
            let text = if r.text.is_empty() {
                crate::display::msgline::say(host, &r.msgid, &[])
            } else {
                r.text
            };
            return Err((r.msgid, text));
        }
    }
    Ok(())
}

/// F1: a message's help when the cursor is on one, else the display's.
pub fn help(host: &Host, surface: &mut Surface, s: &Session, cursor: (usize, usize)) {
    if let Some(Slot::History(k)) = slot_at(s, cursor)
        && let Some(Line::Message { id, text }) = s.h.lines().get(k)
        && !id.is_empty()
    {
        message_help(host, surface, id, text);
        return;
    }
    let lines: Vec<String> = HELP.iter().map(|l| (*l).to_string()).collect();
    super::services::help_display(surface, "Enter SQL Statements - Help", &lines);
}

/// The display's help text.
const HELP: &[&str] = &[
    "Type a statement on the input lines and press Enter to run it. A",
    "statement may run over several lines; the lines are read end to end.",
    "",
    "What ran stays above the input with the messages it left. Page Up",
    "and Page Down move through the session. With the cursor on a message,",
    "F1 shows more about it; on a statement, F9 copies it to the input.",
    "",
    "F4 prompts the statement typed, or lists the statements that can be",
    "prompted. F13 changes the session's attributes and saves or prints",
    "the session. F16, F17 and F18 add library, file and field names",
    "where the cursor is.",
];

/// Additional Message Information for a message of the history.
fn message_help(host: &Host, surface: &mut Surface, id: &str, text: &str) {
    let desc = host
        .link
        .borrow_mut()
        .with(|l| l.describe(id.to_string(), "QSQLMSG".into(), "QSYS".into()));
    let Ok(mut d) = crate::panels::dialog(host, "QUIDSPMSG") else {
        return;
    };
    let user = host.link.borrow_mut().with(Client::whoami).user;
    d.set("FROMUSR", &user);
    d.set("MSGTYPE", "Diagnostic");
    d.set(
        "MSGSEV",
        &format!("{:02}", desc.as_ref().map_or(30, |m| m.severity)),
    );
    d.set("SNTDATE", "");
    d.set("SNTTIME", "");
    d.set("REPLY", "");
    d.clear_list("INFO");
    let mut lines = vec![format!("Message ID  . . :   {id}"), String::new()];
    lines.extend(wrapped("Message . . . . :   ", text));
    if let Some(m) = &desc {
        let help = m.format_help(&[]);
        if !help.trim().is_empty() {
            lines.push(String::new());
            lines.extend(wrapped("Cause . . . . . :   ", &help));
        }
    }
    for l in lines {
        d.add_row("INFO", vec![("LINE".into(), l)]);
    }
    let _ = d.show(surface, "ADDMSGI");
}

/// `label` then `text` wrapped under its value column.
fn wrapped(label: &str, text: &str) -> Vec<String> {
    isql::history::wrap(text, 74 - label.len())
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                format!("{label}{l}")
            } else {
                format!("{:w$}{l}", "", w = label.len())
            }
        })
        .collect()
}
