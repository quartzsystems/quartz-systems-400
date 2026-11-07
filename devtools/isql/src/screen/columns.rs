// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays INSERT, UPDATE and CREATE INDEX go on to, listing the
//! table's columns: a value for each to insert or set, a sequence
//! number and order for each key column. Placed as the pub400
//! captures `strsql-prompt-insert-2`, `-update-2` and `-crtidx-2`
//! place them; words from `QSTXTMSG`.

use uim::Screen;
use uim::render::Attr;

use super::stmt::Label;
use super::{blank, keys, message, more, put, title, var};
use crate::assemble::Values;
use crate::print::Words;

/// The `QSTXTMSG` identifiers.
mod id {
    pub const TITLE: &str = "TXT0A01";
    pub const INFO: &str = "TXT0A02";
    pub const VALUES: &str = "TXT0A06";
    pub const SEQUENCE: &str = "TXT0A07";
    pub const ORDER_KEY: &str = "TXT0A08";
    pub const VALUE: &str = "TXT0A19";
    pub const FIELD: &str = "TXT0A40";
    pub const COLUMN: &str = "TXT0A41";
    pub const SEQ_FIELD: &str = "TXT0A42";
    pub const SEQ_COLUMN: &str = "TXT0A43";
    pub const ORDER: &str = "TXT0A44";
}

/// Which list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    /// INSERT's values, rows 6 to 20.
    Insert,
    /// UPDATE's values under its first display, rows 14 to 20.
    Update,
    /// CREATE INDEX's key, two columns of rows 7 to 20.
    Key,
}

impl List {
    /// The columns a page shows.
    #[must_use]
    pub fn per_page(self) -> usize {
        match self {
            Self::Insert => 15,
            Self::Update => 7,
            Self::Key => 28,
        }
    }

    /// The pages `n` columns take.
    #[must_use]
    pub fn pages(self, n: usize) -> usize {
        n.div_ceil(self.per_page()).max(1)
    }
}

/// The value field of column `i` (from 0): `VALn`.
#[must_use]
pub fn value_name(i: usize) -> String {
    format!("VAL{}", i + 1)
}

/// The sequence number field of column `i`: `SEQn`.
#[must_use]
pub fn seq_name(i: usize) -> String {
    format!("SEQ{}", i + 1)
}

/// The order field of column `i`: `ORDn`.
#[must_use]
pub fn order_name(i: usize) -> String {
    format!("ORD{}", i + 1)
}

/// The fields page `page` shows.
#[must_use]
pub fn names(list: List, n: usize, page: usize) -> Vec<String> {
    let per = list.per_page();
    let range = (page * per).min(n)..((page + 1) * per).min(n);
    match list {
        List::Insert | List::Update => range.map(value_name).collect(),
        List::Key => range.flat_map(|i| [seq_name(i), order_name(i)]).collect(),
    }
}

fn pick(sql: bool, sys: &str, sqln: &str, w: &Words<'_>) -> String {
    w(if sql { sqln } else { sys })
}

/// Rows `from` on: each column's name and its value field.
fn values(s: &mut Screen, from: usize, columns: &[String], page: usize, per: usize, v: &Values) {
    for (k, (i, c)) in columns
        .iter()
        .enumerate()
        .skip(page * per)
        .take(per)
        .enumerate()
    {
        put(s, from + k, 2, Attr::Normal, c);
        let name = value_name(i);
        var(
            s,
            from + k,
            24,
            53,
            &name,
            v.get(&name).map_or("", String::as_str),
        );
    }
}

/// The function keys: row, column, identifier.
fn show_keys(s: &mut Screen, list: &[(usize, usize, Label)], sql: bool, w: &Words<'_>) {
    let shown: Vec<(usize, usize, String)> = list
        .iter()
        .map(|(r, c, l)| (*r, *c, pick(sql, l.sys, l.sql, w)))
        .collect();
    let refs: Vec<(usize, usize, &str)> =
        shown.iter().map(|(r, c, t)| (*r, *c, t.as_str())).collect();
    keys(s, &refs);
}

const fn k(row: usize, col: usize, id: &'static str) -> (usize, usize, Label) {
    (row, col, Label { sys: id, sql: id })
}

const INSERT_KEYS: &[(usize, usize, Label)] = &[
    k(22, 2, "TXT0A45"),
    k(22, 15, "TXT0A47"),
    k(22, 28, "TXT0A48"),
    k(22, 45, "TXT0A49"),
    k(22, 61, "TXT0A4A"),
    k(23, 2, "TXT0A4B"),
    k(23, 15, "TXT0A4C"),
    k(23, 33, "TXT0A4D"),
    k(23, 50, "TXT0A52"),
];

const UPDATE_KEYS: &[(usize, usize, Label)] = &[
    k(22, 2, "TXT0A45"),
    k(22, 12, "TXT0A46"),
    k(22, 24, "TXT0A47"),
    k(22, 37, "TXT0A48"),
    k(22, 55, "TXT0A49"),
    k(23, 2, "TXT0A4A"),
    k(23, 24, "TXT0A4B"),
    k(23, 37, "TXT0A4C"),
    k(23, 55, "TXT0A52"),
];

const KEY_KEYS: &[(usize, usize, Label)] = &[
    k(22, 2, "TXT0A45"),
    k(22, 12, "TXT0A47"),
    k(22, 25, "TXT0A49"),
    k(22, 41, "TXT0A4B"),
    (
        23,
        2,
        Label {
            sys: "TXT0A4E",
            sql: "TXT0A4F",
        },
    ),
    k(23, 35, "TXT0A50"),
    k(23, 61, "TXT0A52"),
];

/// Specify INSERT Statement's values, page `page`.
#[must_use]
pub fn insert(
    columns: &[String],
    v: &Values,
    page: usize,
    sql: bool,
    msg: Option<&str>,
    w: &Words<'_>,
) -> Screen {
    let mut s = blank();
    let heading = w(id::TITLE).replace("&1", "INSERT");
    title(
        &mut s,
        (80 - heading.chars().count().min(80)) / 2 + 1,
        &heading,
    );
    put(&mut s, 3, 2, Attr::Blue, &w(id::VALUES));
    put(
        &mut s,
        5,
        2,
        Attr::HighIntensity,
        &pick(sql, id::FIELD, id::COLUMN, w),
    );
    put(&mut s, 5, 24, Attr::HighIntensity, &w(id::VALUE));
    let per = List::Insert.per_page();
    values(&mut s, 6, columns, page, per, v);
    more(&mut s, 21, 79, (page + 1) * per >= columns.len());
    show_keys(&mut s, INSERT_KEYS, sql, w);
    message(&mut s, msg);
    s.cursor = Some((6, 24));
    s
}

/// Specify UPDATE Statement with the values under `first` (the first
/// display, its keys taken off), page `page`.
#[must_use]
pub fn update(
    first: Screen,
    columns: &[String],
    v: &Values,
    page: usize,
    sql: bool,
    msg: Option<&str>,
    w: &Words<'_>,
) -> Screen {
    let mut s = first;
    s.texts.retain(|t| t.row < 22);
    put(&mut s, 11, 2, Attr::Blue, &w(id::INFO));
    put(
        &mut s,
        13,
        2,
        Attr::HighIntensity,
        &pick(sql, id::FIELD, id::COLUMN, w),
    );
    put(&mut s, 13, 24, Attr::HighIntensity, &w(id::VALUE));
    let per = List::Update.per_page();
    values(&mut s, 14, columns, page, per, v);
    more(&mut s, 21, 79, (page + 1) * per >= columns.len());
    show_keys(&mut s, UPDATE_KEYS, sql, w);
    message(&mut s, msg);
    s
}

/// Specify CREATE INDEX Statement's key columns, page `page`: rows 7
/// to 20 down the left, then the right.
#[must_use]
pub fn key(
    columns: &[String],
    v: &Values,
    page: usize,
    sql: bool,
    msg: Option<&str>,
    w: &Words<'_>,
) -> Screen {
    let mut s = blank();
    let heading = w(id::TITLE).replace("&1", "CREATE INDEX");
    title(
        &mut s,
        (80 - heading.chars().count().min(80)) / 2 + 1,
        &heading,
    );
    put(&mut s, 3, 2, Attr::Blue, &w(id::SEQUENCE));
    put(&mut s, 4, 2, Attr::Blue, &w(id::ORDER_KEY));
    let seq = pick(sql, id::SEQ_FIELD, id::SEQ_COLUMN, w);
    let order = w(id::ORDER);
    for (c, o) in [(2, 29), (43, 70)] {
        put(&mut s, 6, c, Attr::HighIntensity, &seq);
        put(&mut s, 6, o, Attr::HighIntensity, &order);
    }
    let per = List::Key.per_page();
    for (k, (i, c)) in columns
        .iter()
        .enumerate()
        .skip(page * per)
        .take(per)
        .enumerate()
    {
        let (row, left) = (7 + k % 14, if k < 14 { 2 } else { 43 });
        let get = |n: &str| v.get(n).map_or(String::new(), Clone::clone);
        var(&mut s, row, left, 3, &seq_name(i), &get(&seq_name(i)));
        put(&mut s, row, left + 5, Attr::Normal, c);
        var(
            &mut s,
            row,
            left + 29,
            1,
            &order_name(i),
            &get(&order_name(i)),
        );
    }
    more(&mut s, 21, 79, (page + 1) * per >= columns.len());
    show_keys(&mut s, KEY_KEYS, sql, w);
    message(&mut s, msg);
    s.cursor = Some((7, 2));
    s
}

/// INSERT's columns and values: the columns given a value, in order.
#[must_use]
pub fn insert_values(columns: &[String], v: &Values) -> (Vec<String>, Vec<String>) {
    columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let val = v.get(&value_name(i)).map_or("", |s| s.trim());
            (!val.is_empty()).then(|| (c.clone(), val.to_string()))
        })
        .unzip()
}

/// The key: the columns numbered, by number, `DESC` for `D`.
#[must_use]
pub fn key_columns(columns: &[String], v: &Values) -> Vec<String> {
    let mut picked: Vec<(u32, usize, String)> = columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let n: u32 = v.get(&seq_name(i))?.trim().parse().ok()?;
            let desc = v
                .get(&order_name(i))
                .is_some_and(|o| o.trim().eq_ignore_ascii_case("D"));
            Some((n, i, if desc { format!("{c} DESC") } else { c.clone() }))
        })
        .collect();
    picked.sort();
    picked.into_iter().map(|(_, _, c)| c).collect()
}
