// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! F16, F17 and F18: the libraries, files and fields to choose from,
//! the ones numbered put into the statement where the cursor was. The
//! files and fields come from the catalog views through the service;
//! the libraries from the session's library option.

use isql::screen::lists::{self, Entry, Which, chosen, page};
use stg::Client;

use super::ui::{Key, show};
use super::{MAX_ROWS, Session, insert_at};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// A list until Enter (the entries numbered) or F12/F3 (`None`).
fn choose(
    surface: &mut Surface,
    which: Which,
    mut entries: Vec<Entry>,
    sql: bool,
    note: Option<String>,
) -> Option<Vec<Entry>> {
    let mut top = 0;
    let per = page(which);
    let mut msg = note;
    if entries.is_empty() {
        msg = Some("There is nothing to list.".into());
    }
    loop {
        let screen = lists::list(which, &entries, top, sql, msg.take().as_deref());
        let a = show(surface, &screen)?;
        for e in entries.iter_mut().skip(top).take(per) {
            e.seq.clear();
        }
        for (i, v) in &a.options {
            if let Some(e) = entries.get_mut(*i) {
                e.seq.clone_from(v);
            }
        }
        match a.key {
            Key::F(3 | 12) => return None,
            Key::PageDown if top + per < entries.len() => top += per,
            Key::PageUp => top = top.saturating_sub(per),
            Key::PageDown => {}
            Key::Enter => {
                let picked: Vec<Entry> = chosen(&entries).into_iter().cloned().collect();
                if picked.is_empty() {
                    msg = Some("Type a sequence number beside at least one entry.".into());
                    continue;
                }
                return Some(picked);
            }
            Key::F(5) => {
                for e in &mut entries {
                    e.seq.clear();
                }
            }
            _ => msg = Some("That function key is not used on this display.".into()),
        }
    }
}

/// The libraries the session's library option names.
fn libraries(host: &Host, s: &Session) -> Vec<Entry> {
    let opt = s.attrs.get("LIBOPT");
    let l = host.attrs().libl;
    let names: Vec<String> = match opt.as_str() {
        "*LIBL" => l
            .sys
            .iter()
            .chain(l.prd.iter().filter(|n| !n.is_empty()))
            .chain(l.cur.iter())
            .chain(l.usr.iter())
            .cloned()
            .collect(),
        "*USRLIBL" => l.cur.iter().chain(l.usr.iter()).cloned().collect(),
        "*CURLIB" => l.cur.iter().cloned().collect(),
        "*ALL" | "*ALLUSR" => host
            .link
            .borrow_mut()
            .with(|c| c.list_objects("QSYS".into(), "*ALL".into(), "*LIB".into()))
            .into_iter()
            .map(|o| o.name.trim().to_string())
            .filter(|n| opt == "*ALL" || !n.starts_with('Q'))
            .collect(),
        name => vec![name.to_string()],
    };
    names
        .into_iter()
        .map(|n| {
            let text = host
                .link
                .borrow_mut()
                .with(|c| c.object("QSYS".into(), n.clone(), "*LIB".into()))
                .map(|o| o.text)
                .unwrap_or_default();
            Entry {
                name: n,
                kind: "SYS".into(),
                text,
                ..Entry::default()
            }
        })
        .collect()
}

/// A catalog query's rows, each column as text.
fn catalog(host: &Host, query: String) -> Vec<Vec<String>> {
    let r = host
        .link
        .borrow_mut()
        .with(|l| l.isql_run(query, "*SQL".into(), false, MAX_ROWS));
    r.rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|v| v.unwrap_or_default().trim().to_string())
                .collect()
        })
        .collect()
}

/// `'A', 'B'` for an `IN` list.
fn quoted(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("'{}'", n.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The files of the chosen libraries.
fn files(host: &Host, libs: &[String]) -> Vec<Entry> {
    let q = format!(
        "SELECT SYSTEM_TABLE_NAME, SYSTEM_TABLE_SCHEMA, TABLE_TYPE, TABLE_TEXT \
         FROM QSYS2.SYSTABLES WHERE SYSTEM_TABLE_SCHEMA IN ({}) \
         ORDER BY SYSTEM_TABLE_SCHEMA, SYSTEM_TABLE_NAME",
        quoted(libs)
    );
    catalog(host, q)
        .into_iter()
        .map(|r| {
            let kind = match r.get(2).map(String::as_str) {
                Some("T") => "TB",
                Some("P") => "PF",
                Some("L") => "LF",
                Some("V") => "VW",
                Some(other) => other,
                None => "",
            }
            .to_string();
            Entry {
                name: r.first().cloned().unwrap_or_default(),
                owner: r.get(1).cloned().unwrap_or_default(),
                kind,
                text: r.get(3).cloned().unwrap_or_default(),
                ..Entry::default()
            }
        })
        .collect()
}

/// The fields of the chosen files.
fn fields(host: &Host, files: &[(String, String)]) -> Vec<Entry> {
    let mut out = Vec::new();
    for (lib, file) in files {
        let q = format!(
            "SELECT COLUMN_NAME, SYSTEM_TABLE_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE \
             FROM QSYS2.SYSCOLUMNS WHERE SYSTEM_TABLE_SCHEMA = {} \
             AND SYSTEM_TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
            quoted(std::slice::from_ref(lib)),
            quoted(std::slice::from_ref(file))
        );
        out.extend(catalog(host, q).into_iter().map(|r| {
            Entry {
                name: r.first().cloned().unwrap_or_default(),
                owner: r.get(1).cloned().unwrap_or_default(),
                kind: r.get(2).cloned().unwrap_or_default(),
                length: r.get(3).cloned().unwrap_or_default(),
                scale: r
                    .get(4)
                    .cloned()
                    .filter(|x| !x.is_empty() && x != "0")
                    .unwrap_or_default(),
                ..Entry::default()
            }
        }));
    }
    out
}

/// The library list step, kept in the session.
fn pick_libraries(
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    note: Option<String>,
) -> Option<()> {
    let sql = s.sql();
    let picked = choose(surface, Which::Libraries, libraries(host, s), sql, note)?;
    s.libs = picked.into_iter().map(|e| e.name).collect();
    Some(())
}

/// The file list step, kept in the session.
fn pick_files(
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    note: Option<String>,
) -> Option<()> {
    let sql = s.sql();
    let picked = choose(surface, Which::Files, files(host, &s.libs), sql, note)?;
    s.files = picked.into_iter().map(|e| (e.owner, e.name)).collect();
    Some(())
}

/// F16, F17 or F18 with the cursor at `at` (input line, column); a
/// message for the main display.
pub fn select(
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    key: u8,
    at: (usize, usize),
) -> Option<String> {
    let sql = s.sql();
    let sep = if sql { '.' } else { '/' };
    let text = match key {
        16 => {
            pick_libraries(host, surface, s, None)?;
            s.libs.join(", ")
        }
        17 => {
            if s.libs.is_empty() {
                let note = Some("Choose the libraries whose files to list.".to_string());
                pick_libraries(host, surface, s, note)?;
            }
            pick_files(host, surface, s, None)?;
            s.files
                .iter()
                .map(|(l, f)| format!("{l}{sep}{f}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
        _ => {
            if s.libs.is_empty() {
                let note = Some("Choose the libraries whose files to list.".to_string());
                pick_libraries(host, surface, s, note)?;
            }
            if s.files.is_empty() {
                let note = Some("Choose the files whose fields to list.".to_string());
                pick_files(host, surface, s, note)?;
            }
            let picked = choose(surface, Which::Fields, fields(host, &s.files), sql, None)?;
            picked
                .into_iter()
                .map(|e| e.name)
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    insert_at(s, at, &format!("{text} "));
    None
}
