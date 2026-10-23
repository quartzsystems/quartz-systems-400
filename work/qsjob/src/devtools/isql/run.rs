// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running the statement typed: the service runs it; a query's rows go
//! to Display Data or to `QSYSPRT`; what happened goes into the
//! history as a message. Also the help behind F1.

use isql::attrs::Output;
use isql::history::Line;
use isql::result::{Column, Data, report};
use isql::screen::data;
use isql::screen::enter::Slot;
use stg::Client;
use stg::proto::IsqlReply;

use super::ui::{Key, show};
use super::{MAX_ROWS, Session, slot_at};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// What a statement that ran says.
fn done_text(r: &IsqlReply) -> String {
    let n = r.count;
    match r.statement.as_str() {
        "INSERT" => format!("{n} rows inserted."),
        "UPDATE" if n == 0 => "No rows were updated.".into(),
        "UPDATE" => format!("{n} rows updated."),
        "DELETE" if n == 0 => "No rows were deleted.".into(),
        "DELETE" => format!("{n} rows deleted."),
        "MERGE" => format!("{n} rows merged."),
        "SELECT" | "EXPLAIN" => format!("SELECT ran; {n} rows."),
        "" => "Statement ran.".into(),
        other => format!("{other} ran."),
    }
}

/// Run the input as a statement.
pub fn statement(host: &Host, surface: &mut Surface, s: &mut Session) {
    let lines = s.h.typed();
    let text = s.h.text();
    let check = s.attrs.syntax_only();
    let r = host
        .link
        .borrow_mut()
        .with(|l| l.isql_run(text.clone(), s.naming(), check, MAX_ROWS));
    s.h.statement(lines.clone());
    if r.sqlcode < 0 {
        s.h.message(&r.msgid, &r.text);
        s.h.input = lines;
        s.h.to_last_statement();
        return;
    }
    if check {
        let what = if s.attrs.get("PROCESS") == "*VLD" {
            "The statement is valid."
        } else {
            "The statement's syntax is correct."
        };
        s.h.message("", what);
    } else if r.columns.is_empty() {
        s.h.message("", &done_text(&r));
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
        };
        let note = r
            .truncated
            .then(|| format!("Only the first {MAX_ROWS} rows are shown."));
        match s.attrs.output() {
            Output::Display => display(surface, &d, note.as_deref()),
            Output::Printer => {
                let lines = report(&text, &d, 132);
                let m = print(host, &lines);
                s.h.message("", &m);
            }
            Output::File => s.h.message(
                "",
                "SELECT output to a file is not available; the rows were not kept.",
            ),
        }
        s.h.message("", &done_text(&r));
    }
    s.h.clear_input();
    s.h.to_end();
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
                    msg = Some("This is the end of the data.".into());
                }
                p.down(rows);
            }
            Key::PageUp => {
                if p.top == 0 {
                    msg = Some("This is the start of the data.".into());
                }
                p.up();
            }
            Key::F(19) => p.left(),
            Key::F(20) => p.right(width),
            Key::F(21) => {
                msg = Some("The split display is not available in this release.".into());
            }
            Key::Enter => {
                if !p.position(&a.get("POSITION"), rows) {
                    msg = Some("Type a line number, T for the top or B for the bottom.".into());
                }
                if !p.shift_to(&a.get("SHIFT"), width) {
                    msg = Some(format!("Type a column from 1 to {width}."));
                }
            }
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// Print lines to `QSYSPRT` as a spooled file; the message to show.
pub fn print(host: &Host, lines: &[String]) -> String {
    use spool::printer::{Control, PageFormat, Printer};
    let opened = host
        .link
        .borrow_mut()
        .with(|l| l.prt_open("QSYSPRT".into(), "*LIBL".into(), "*DFTACTGRP".into()));
    let (id, prtf) = match opened {
        Ok(o) => o,
        Err(e) => return format!("Nothing was printed: {e}"),
    };
    let mut p = Printer::new(PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: prtf.fold,
    });
    for l in lines {
        p.write_str(l, Control::line());
    }
    let bytes = p.take();
    let put = host
        .link
        .borrow_mut()
        .with(|l| l.splf_put(id, bytes, p.pages, p.records));
    let closed = host.link.borrow_mut().with(|l| l.splf_close(id, prtf.hold));
    match put.and(closed) {
        Ok(()) => "The output was printed to QSYSPRT.".into(),
        Err(e) => format!("Nothing was printed: {e}"),
    }
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
