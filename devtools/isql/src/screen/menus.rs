// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small displays around the session: Select SQL Statement (F4 on
//! a blank line), Interactive SQL Session Services (F13), Change
//! Session Attributes and Exit Interactive SQL.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title, var};
use crate::attrs::{Attrs, Output};
use crate::format::{DateFmt, TimeFmt};
use crate::print::Words;

/// The statements Select SQL Statement lists, in its order.
pub const STATEMENTS: &[&str] = &[
    "ALTER TABLE",
    "CALL",
    "COMMENT ON",
    "COMMIT",
    "CONNECT",
    "CREATE ALIAS",
    "CREATE COLLECTION",
    "CREATE INDEX",
    "CREATE PROCEDURE",
    "CREATE TABLE",
    "CREATE VIEW",
    "DELETE",
    "DISCONNECT",
    "DROP ALIAS",
    "DROP COLLECTION",
    "DROP INDEX",
    "DROP PACKAGE",
    "DROP TABLE",
    "DROP VIEW",
    "GRANT PACKAGE",
    "GRANT TABLE",
    "INSERT",
    "LABEL ON",
    "LOCK TABLE",
    "RENAME",
    "RELEASE",
    "REVOKE PACKAGE",
    "REVOKE TABLE",
    "ROLLBACK",
    "SELECT",
    "SET CONNECTION",
    "SET TRANSACTION",
    "UPDATE",
];

/// Statements on a page of the list.
pub const LIST_PAGE: usize = 14;

/// Select SQL Statement, page `page` from 0.
#[must_use]
pub fn statement_list(page: usize, selection: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 31, "Select SQL Statement");
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    let start = page * LIST_PAGE;
    for (i, st) in STATEMENTS.iter().enumerate().skip(start).take(LIST_PAGE) {
        put(
            &mut s,
            5 + i - start,
            5,
            Attr::Normal,
            &format!("{:>3}. {st}", i + 1),
        );
    }
    more(&mut s, 19, 79, start + LIST_PAGE >= STATEMENTS.len());
    put(&mut s, 20, 2, Attr::Normal, "Selection");
    var(&mut s, 21, 7, 2, "SELECTION", selection);
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((21, 7));
    s
}

/// The pages of the statement list.
#[must_use]
pub fn statement_pages() -> usize {
    STATEMENTS.len().div_ceil(LIST_PAGE)
}

/// Interactive SQL Session Services.
#[must_use]
pub fn services(selection: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 27, "Interactive SQL Session Services");
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    for (i, t) in [
        "Change session attributes",
        "Print current session",
        "Remove all entries from current session",
        "Save session in source file",
    ]
    .iter()
    .enumerate()
    {
        put(&mut s, 5 + i, 7, Attr::Normal, &format!("{}. {t}", i + 1));
    }
    put(&mut s, 20, 2, Attr::Normal, "Selection");
    var(&mut s, 21, 7, 2, "SELECTION", selection);
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 14, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((21, 7));
    s
}

/// One attribute on Change Session Attributes: key, field width, the
/// choices shown to its right (a line each; the second column given
/// when it is indented).
struct AttrRow {
    key: &'static str,
    width: usize,
    hints: &'static [(usize, &'static str)],
}

const PAGE1: &[AttrRow] = &[
    AttrRow {
        key: "PROCESS",
        width: 4,
        hints: &[(52, "*RUN, *VLD, *SYN")],
    },
    AttrRow {
        key: "OUTPUT",
        width: 1,
        hints: &[(52, "1=Display, 2=Printer"), (52, "3=File")],
    },
    AttrRow {
        key: "COMMIT",
        width: 5,
        hints: &[(52, "*NONE, *CHG, *CS, *ALL, *RR"), (52, "*NC, *UR, *RS")],
    },
    AttrRow {
        key: "DATFMT",
        width: 4,
        hints: &[
            (52, "*JOB, *USA, *ISO, *EUR, *JIS"),
            (52, "*MDY, *DMY, *YMD, *JUL"),
        ],
    },
    AttrRow {
        key: "DATSEP",
        width: 6,
        hints: &[(52, "*JOB, '/', '.', ',', '-'"), (52, "' ', *BLANK")],
    },
    AttrRow {
        key: "TIMFMT",
        width: 4,
        hints: &[(52, "*HMS, *USA, *ISO"), (52, "*EUR, *JIS")],
    },
    AttrRow {
        key: "TIMSEP",
        width: 6,
        hints: &[(52, "*JOB, ':', '.', ','"), (52, "' ', *BLANK")],
    },
    AttrRow {
        key: "REFRESH",
        width: 8,
        hints: &[(52, "*ALWAYS, *FORWARD")],
    },
    AttrRow {
        key: "ALWCPYDTA",
        width: 9,
        hints: &[(52, "*YES, *OPTIMIZE, *NO")],
    },
    AttrRow {
        key: "NAMING",
        width: 4,
        hints: &[(52, "*SYS, *SQL")],
    },
];

const PAGE2: &[AttrRow] = &[
    AttrRow {
        key: "LIBOPT",
        width: 10,
        hints: &[
            (52, "Name, *LIBL, *USRLIBL"),
            (52, "*ALLUSR, *ALL, *CURLIB"),
        ],
    },
    AttrRow {
        key: "LISTTYPE",
        width: 4,
        hints: &[(52, "*ALL, *SQL")],
    },
    AttrRow {
        key: "DECPNT",
        width: 7,
        hints: &[(52, "*PERIOD, *COMMA, *JOB,"), (54, "*SYSVAL")],
    },
    AttrRow {
        key: "SRTSEQ",
        width: 10,
        hints: &[
            (52, "Name, *JOB, *JOBRUN"),
            (52, "*LANGIDUNQ, *LANGIDSHR"),
            (52, "*HEX"),
        ],
    },
    AttrRow {
        key: "LANGID",
        width: 7,
        hints: &[(52, "Name, *JOB, *JOBRUN"), (52, "F4 for Prompt")],
    },
    AttrRow {
        key: "SQLCURRULE",
        width: 4,
        hints: &[(52, "*DB2, *STD")],
    },
    AttrRow {
        key: "DSPPWD",
        width: 4,
        hints: &[(52, "*NO, *YES")],
    },
];

/// The rows Change Session Attributes lays out from: row 5 to 21.
const FIRST_ROW: usize = 5;
/// The last row items are placed on.
const LAST_ROW: usize = 21;

/// One placed piece of Change Session Attributes: a heading line, or
/// an attribute's label, field and choices.
#[derive(Clone, Debug)]
struct Placed {
    /// The attribute; `None` for a heading.
    key: Option<&'static str>,
    /// The label's column.
    col: usize,
    /// The label.
    label: String,
    /// The field's column and width.
    field: (usize, usize),
    /// The choices, a row each.
    hints: Vec<(usize, String)>,
    /// Rows taken.
    height: usize,
}

impl Placed {
    fn row(r: &AttrRow) -> Self {
        Self {
            key: Some(r.key),
            col: 4,
            label: crate::attrs::field(r.key)
                .map_or("", |f| f.label)
                .to_string(),
            field: (37, r.width),
            hints: r
                .hints
                .iter()
                .map(|(c, h)| (*c, (*h).to_string()))
                .collect(),
            height: r.hints.len().max(1),
        }
    }

    fn word(
        key: &'static str,
        col: usize,
        label: &str,
        field: (usize, usize),
        hints: &[&str],
        w: &Words<'_>,
    ) -> Self {
        Self {
            key: Some(key),
            col,
            label: label.to_string(),
            field,
            hints: hints.iter().map(|h| (52, w(h))).collect(),
            height: hints.len().max(1) + usize::from(field.0 + field.1 > 81),
        }
    }
}

/// The `QSTXTMSG` words of the output rows.
mod words {
    pub const OUTPUT_FILE: &str = "TXT0934";
    pub const NAME: &str = "TXT0935";
    pub const MEMBERS: &str = "TXT0936";
    pub const CREATE_FILE: &str = "TXT0937";
    pub const REPLACE_FILE: &str = "TXT0938";
    pub const CREATE_MEMBER: &str = "TXT0939";
    pub const REPLACE_MEMBER: &str = "TXT093A";
    pub const ADD_MEMBER: &str = "TXT093B";
    pub const AUTL: &str = "TXT093C";
    pub const AUTS: &str = "TXT093D";
    pub const AUTS2: &str = "TXT093E";
}

/// The attributes `a` shows, in order: the printer under SELECT output
/// 2, the output file under 3, a date separator only for the formats
/// that take one, a time separator only for `*HMS`.
fn items(a: &Attrs, w: &Words<'_>) -> Vec<Placed> {
    let label = |key: &str| {
        let l = crate::attrs::field(key).map_or("", |f| f.label);
        if l.starts_with("TXT") {
            w(l)
        } else {
            l.to_string()
        }
    };
    let mut out = Vec::new();
    for r in PAGE1.iter().chain(PAGE2) {
        let keep = match r.key {
            "DATSEP" => DateFmt::parse(&a.get("DATFMT")).is_none_or(DateFmt::has_sep),
            "TIMSEP" => TimeFmt::parse(&a.get("TIMFMT")).is_none_or(TimeFmt::has_sep),
            _ => true,
        };
        if keep {
            out.push(Placed::row(r));
        }
        if r.key != "OUTPUT" {
            continue;
        }
        match a.output() {
            Output::Printer => out.push(Placed::word(
                "PRINTER",
                4,
                &label("PRINTER"),
                (37, 10),
                &[words::NAME],
                w,
            )),
            Output::File => {
                out.push(Placed {
                    key: None,
                    col: 4,
                    label: w(words::OUTPUT_FILE),
                    field: (0, 0),
                    hints: Vec::new(),
                    height: 1,
                });
                out.push(Placed::word(
                    "OUTFILE",
                    6,
                    &label("OUTFILE"),
                    (37, 10),
                    &[words::NAME],
                    w,
                ));
                out.push(Placed::word(
                    "OUTLIB",
                    8,
                    &label("OUTLIB"),
                    (39, 10),
                    &[words::NAME],
                    w,
                ));
                out.push(Placed::word(
                    "OUTMBR",
                    6,
                    &label("OUTMBR"),
                    (37, 10),
                    &[words::MEMBERS],
                    w,
                ));
                out.push(Placed::word(
                    "OUTOPT",
                    6,
                    &label("OUTOPT"),
                    (37, 1),
                    &[
                        words::CREATE_FILE,
                        words::REPLACE_FILE,
                        words::CREATE_MEMBER,
                        words::REPLACE_MEMBER,
                        words::ADD_MEMBER,
                    ],
                    w,
                ));
                out.push(Placed::word(
                    "OUTAUT",
                    6,
                    &label("OUTAUT"),
                    (37, 10),
                    &[words::AUTL, words::AUTS, words::AUTS2],
                    w,
                ));
                out.push(Placed::word(
                    "OUTTEXT",
                    6,
                    &label("OUTTEXT"),
                    (37, 50),
                    &[],
                    w,
                ));
            }
            Output::Display => {}
        }
    }
    out
}

/// The items cut into pages of rows 5 to 21, none split.
fn pages_of(items: Vec<Placed>) -> Vec<Vec<Placed>> {
    let room = LAST_ROW + 1 - FIRST_ROW;
    let mut pages: Vec<Vec<Placed>> = vec![Vec::new()];
    let mut used = 0;
    for it in items {
        if used + it.height > room && used > 0 {
            pages.push(Vec::new());
            used = 0;
        }
        used += it.height;
        if let Some(p) = pages.last_mut() {
            p.push(it);
        }
    }
    pages
}

/// The pages of Change Session Attributes for `a`.
#[must_use]
pub fn attr_pages(a: &Attrs, w: &Words<'_>) -> usize {
    pages_of(items(a, w)).len()
}

/// The attribute keys page `page` shows, for reading the fields back.
#[must_use]
pub fn attr_keys(page: usize, a: &Attrs, w: &Words<'_>) -> Vec<&'static str> {
    pages_of(items(a, w))
        .get(page)
        .map(|p| p.iter().filter_map(|i| i.key).collect())
        .unwrap_or_default()
}

/// Change Session Attributes, page `page` from 0.
#[must_use]
pub fn attributes(page: usize, a: &Attrs, msg: Option<&str>, w: &Words<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 28, "Change Session Attributes");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    let pages = pages_of(items(a, w));
    let page = page.min(pages.len() - 1);
    let mut row = FIRST_ROW;
    let mut first = None;
    for it in &pages[page] {
        match it.key {
            None => put(&mut s, row, it.col, Attr::Normal, &it.label),
            Some(key) => {
                put(
                    &mut s,
                    row,
                    it.col,
                    Attr::Normal,
                    &leader(&it.label, it.col, 33),
                );
                var(&mut s, row, it.field.0, it.field.1, key, &a.shown(key));
                first.get_or_insert((row, it.field.0));
                for (i, (col, h)) in it.hints.iter().enumerate() {
                    put(&mut s, row + i, *col, Attr::Normal, h);
                }
            }
        }
        row += it.height;
    }
    more(&mut s, 22, 79, page + 1 == pages.len());
    keys(
        &mut s,
        &[
            (23, 2, "F3=Exit"),
            (23, 12, "F4=Prompt"),
            (23, 24, "F5=Refresh"),
            (23, 37, "F12=Cancel"),
        ],
    );
    message(&mut s, msg);
    s.cursor = first;
    s
}

/// Exit Interactive SQL with the option as typed.
#[must_use]
pub fn exit(option: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 31, "Exit Interactive SQL");
    put(&mut s, 3, 2, Attr::Blue, "Type choice, press Enter.");
    put(&mut s, 5, 4, Attr::Normal, &leader("Option", 4, 27));
    var(&mut s, 5, 31, 7, "OPTION", option);
    for (i, t) in [
        "1=Save and exit session",
        "2=Exit without saving session",
        "3=Resume session",
        "4=Save session in source file",
    ]
    .iter()
    .enumerate()
    {
        put(&mut s, 5 + i, 39, Attr::Normal, t);
    }
    keys(&mut s, &[(23, 2, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((5, 31));
    s
}
