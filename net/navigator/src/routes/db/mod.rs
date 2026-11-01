// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database: `/db[/<schema>[/<table>/...]]` from the catalog views,
//! `/sql` one statement run as the user. Run SQL's statements and last
//! result are Navigator's own state, kept in memory per user
//! ([`SqlState`]).

mod schemas;
mod sql;
mod table;

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::Mutex;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Cell, Col, List, list_table, msgid};

use super::Rq;
use crate::PRODUCT;
use crate::env::db::{SqlOptions, SqlOutcome};

/// The tree's group.
const GROUP: &str = "Database";

/// The most rows a result shows.
pub const MAX_ROWS: usize = 500;

/// Statements Run SQL keeps per user.
pub const HISTORY: usize = 50;

/// What Run SQL last did for a user.
#[derive(Clone, Debug, Default)]
pub struct Last {
    /// The statement.
    pub statement: String,
    /// How it ran.
    pub opts: SqlOptions,
    /// What it left, when it ran.
    pub outcome: Option<SqlOutcome>,
    /// Its access plan, when explained.
    pub plan: Option<Result<Vec<String>, Message>>,
}

/// One user's Run SQL state.
#[derive(Clone, Debug, Default)]
pub struct UserSql {
    /// Statements run, newest first.
    pub history: VecDeque<String>,
    /// The last statement and its result.
    pub last: Option<Last>,
}

/// Run SQL's state of every user.
#[derive(Default)]
pub struct SqlState {
    users: Mutex<BTreeMap<String, UserSql>>,
}

impl SqlState {
    /// `user`'s state.
    pub fn of(&self, user: &str) -> UserSql {
        self.users.lock().expect("sql").get(user).cloned().unwrap_or_default()
    }

    /// Change `user`'s state.
    pub fn update(&self, user: &str, f: impl FnOnce(&mut UserSql)) {
        f(self.users.lock().expect("sql").entry(user.to_string()).or_default());
    }

    /// Note a statement run: newest first, each once, at most
    /// [`HISTORY`].
    pub fn remember(&self, user: &str, statement: &str) {
        self.update(user, |u| {
            u.history.retain(|s| s != statement);
            u.history.push_front(statement.to_string());
            u.history.truncate(HISTORY);
        });
    }
}

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["sql"] => sql::page(rq),
        ["db"] => schemas::page(rq, None),
        ["db", s] => schemas::page(rq, Some(s)),
        ["db", s, t] => rq.redirect(&format!("/db/{s}/{t}/columns")),
        ["db", s, t, "data"] => table::data(rq, s, t),
        ["db", s, t, tab] => table::tab(rq, s, t, tab),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A page of the Database group.
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    super::common::group_page(rq, GROUP, crumb, h1, desc, body, help, item)
}

/// A result value as shown.
fn shown(v: Option<&String>) -> String {
    v.map_or_else(|| "NULL".to_string(), Clone::clone)
}

/// Sort rows by column `key` (a column name in lower case).
#[allow(clippy::many_single_char_names)]
fn sort_rows(columns: &[String], rows: &mut [Vec<Option<String>>], key: &str) {
    let Some(i) = columns.iter().position(|c| c.to_ascii_lowercase() == key) else {
        return;
    };
    rows.sort_by(|a, b| {
        let (x, y) = (a[i].as_deref().unwrap_or(""), b[i].as_deref().unwrap_or(""));
        match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
            (Ok(p), Ok(q)) => p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal),
            _ => x.cmp(y),
        }
    });
}

/// A result table, every column sortable; `keep` the query values the
/// sort links keep.
fn result_table(columns: &[String], rows: &[Vec<Option<String>>], keep: Vec<(String, String)>) -> String {
    let list = List {
        cols: columns.iter().map(|c| Col::sorted(c, &c.to_ascii_lowercase())).collect(),
        rows: rows
            .iter()
            .map(|r| r.iter().map(|v| Cell::mono(&shown(v.as_ref()))).collect())
            .collect(),
        empty: "The result is empty.".into(),
        keep,
    };
    list_table(&list)
}

/// `12 rows returned (maximum 500).` and the CSV link.
fn returned(n: usize, csv: &str) -> String {
    format!(
        "<div class=\"small\">{n} {} returned (maximum {MAX_ROWS}). <a href=\"{}\">Download as CSV</a></div>",
        if n == 1 { "row" } else { "rows" },
        esc(csv)
    )
}

/// An outcome's messages: for a query, the rows fetched and the end of
/// the data, then the statement's own.
fn messages(rq: &Rq<'_>, o: &SqlOutcome) -> Vec<Message> {
    let mut out = Vec::new();
    if o.query && !o.failed() {
        let secs = format!("{}.{:03}", o.elapsed_ms / 1000, o.elapsed_ms % 1000);
        out.push(rq.message(Kind::Done, "SQL7958", &[&o.rows.len().to_string(), &secs]));
        if !o.truncated {
            out.push(rq.message(Kind::Info, "SQL0100", &["FETCH"]));
        }
    }
    out.extend(o.messages.iter().cloned());
    out
}

/// The `Messages:` line.
fn message_line(ms: &[Message]) -> String {
    let mut s = String::from("<div class=\"small\"><b>Messages:</b> ");
    for (i, m) in ms.iter().enumerate() {
        if i > 0 {
            s.push_str(" &nbsp; ");
        }
        let _ = write!(s, "{} - {}", msgid(&PRODUCT, &m.id), esc(&m.text));
    }
    s.push_str("</div>");
    s
}

/// Rows as CSV (RFC 4180): a heading line, nulls empty.
pub fn csv(columns: &[String], rows: &[Vec<Option<String>>]) -> Vec<u8> {
    fn field(s: &str) -> String {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
    let mut out = String::new();
    let line = |vals: Vec<String>| vals.iter().map(|v| field(v)).collect::<Vec<_>>().join(",") + "\r\n";
    out.push_str(&line(columns.to_vec()));
    for r in rows {
        out.push_str(&line(r.iter().map(|v| v.clone().unwrap_or_default()).collect()));
    }
    out.into_bytes()
}

/// Size in megabytes to a tenth (`88.1`).
fn mb10(bytes: u64) -> String {
    let t = (u128::from(bytes) * 10 + (1 << 19)) >> 20;
    format!("{}.{}", t / 10, t % 10)
}
