// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database: `/db/tables` (the catalog), `/db/<schema>/<table>` (rows,
//! insert, edit, delete), `/sql` (Run SQL), `/requests` (saved
//! requests), `/db/copy/to|from` (Data Transfer). Everything runs as
//! the signed-on user through [`crate::env::db::Db`], gated by the
//! database policies.

#![allow(clippy::many_single_char_names)] // rows, schemas, tables

mod copy;
mod record;
mod requests;
mod sql;
mod state;
mod tables;
mod view;

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::Cell;

use super::Rq;
use qsweb::msg::{Kind as MsgKind, Message};

use crate::env::db::{Commit, Options, Outcome, Val};
use crate::prefs;

/// The tree group.
pub const GROUP: &str = "Database";

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    state::prune(rq);
    match segs {
        ["db", "tables"] => tables::page(rq),
        ["db", "copy", "to"] => copy::to(rq),
        ["db", "copy", "from"] => copy::from(rq),
        ["db", s, t] => view::page(rq, s, t),
        ["db", s, t, "insert"] => record::insert(rq, s, t),
        ["db", s, t, "edit"] => record::edit(rq, s, t),
        ["db", s, t, "delete", "confirm"] => view::delete(rq, s, t),
        ["sql"] => sql::page(rq),
        ["sql", "result", n] => sql::result(rq, n),
        ["requests", rest @ ..] => requests::route(rq, rest),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A select whose first option stands for the default: it is not
/// marked, as the pages show it.
pub fn pick(name: &str, options: &[String], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && o == chosen { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// [`pick`] of string slices.
pub fn picks(name: &str, options: &[&str], chosen: &str) -> String {
    pick(name, &options.iter().map(ToString::to_string).collect::<Vec<_>>(), chosen)
}

/// A value as a list cell: numbers right-aligned, NULL a grey `-`,
/// binary data as hex (or as text when `hex_as_text`).
pub fn cell(v: &Val, numeric: bool) -> Cell {
    match v {
        Val::Null => Cell::mono_html("<span class=\"grey\">-</span>".into()),
        Val::Num(n) => Cell::num(n),
        v if numeric => Cell::num(&v.text()),
        v => Cell::mono(&v.text()),
    }
}

/// `MM/DD/YY HH:MM` as the user reads dates; blank for 0.
pub fn short_stamp(rq: &Rq<'_>, secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let (d, t) = rq.stamp(secs);
    // Seconds off, AM/PM kept.
    let (hm, rest) = t.split_at(t.len().min(5));
    let ampm = rest.get(3..).unwrap_or("");
    format!("{d} {hm}{ampm}")
}

/// A page under Navigator for QS/400 on this host (its port: 2002, or
/// 2005 over TLS), as the handoff has the product link to it.
// BLOCKED(rel): without QS00NAV installed the link has no page; the handoff's Columns page embedded in this product is not built.
pub fn navigator(rq: &Rq<'_>, route: &str) -> String {
    let host = rq.req.header("host").unwrap_or("localhost");
    let name = if host.starts_with('[') {
        host.split_inclusive(']').next().unwrap_or(host)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let (scheme, port) = if rq.peer.secure { ("https", 2005) } else { ("http", 2002) };
    format!("{scheme}://{name}:{port}/qs400/navigator{route}")
}

/// The user's connection options from a form (or the preferences).
pub fn options(rq: &Rq<'_>, schema: &str, naming: &str, commit: &str) -> Options {
    let p = rq.prefs();
    Options {
        schema: if schema.is_empty() { p.get(prefs::SCHEMA) } else { schema.to_string() },
        system_naming: if naming.is_empty() { p.get(prefs::NAMING) == "sys" } else { naming == "System" },
        commit: Commit::of(commit).unwrap_or_default(),
    }
}

/// Run `f` over a connection of its own, closed after it (uncommitted
/// changes roll back).
pub fn with_conn<T>(rq: &Rq<'_>, f: impl FnOnce(u32) -> T) -> Result<T, Message> {
    let db = &rq.env().db;
    let c = db.open(rq.user())?;
    let r = f(c);
    db.close(rq.user(), c);
    Ok(r)
}

/// A failed outcome's message (the database's).
pub fn failure(o: &Outcome) -> Option<Message> {
    o.failed.then(|| {
        o.message.clone().unwrap_or_else(|| {
            Message::new(MsgKind::Error, "SQL0901", &format!("The statement failed with SQLSTATE {}.", o.sqlstate))
        })
    })
}

/// The crumb of a database page.
pub fn crumb(parts: &[&str]) -> Vec<String> {
    std::iter::once(GROUP).chain(parts.iter().copied()).map(ToString::to_string).collect()
}

/// A number with thousands separators (`2,207`).
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A name as the catalog keeps it: upper case unless delimited.
pub fn name_of(s: &str) -> String {
    let t = s.trim();
    match t.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        Some(q) => q.replace("\"\"", "\""),
        None => t.to_ascii_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(thousands(2207), "2,207");
        assert_eq!(thousands(412), "412");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(name_of(" assets "), "ASSETS");
        assert_eq!(name_of("\"Mixed\""), "Mixed");
        assert!(pick("s", &["A".into(), "B".into()], "A").contains("<option>A</option>"));
        assert!(pick("s", &["A".into(), "B".into()], "B").contains("<option selected>B</option>"));
    }
}
