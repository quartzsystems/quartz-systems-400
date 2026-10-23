// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small displays around the session: Select SQL Statement (F4 on
//! a blank line), Interactive SQL Session Services (F13), Change
//! Session Attributes and Exit Interactive SQL.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title, var};
use crate::attrs::Attrs;

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

/// One attribute on Change Session Attributes: key, row, field width,
/// the choices shown to its right (a line each; the second column
/// given when it is indented).
struct AttrRow {
    key: &'static str,
    row: usize,
    width: usize,
    hints: &'static [(usize, &'static str)],
}

const PAGE1: &[AttrRow] = &[
    AttrRow {
        key: "PROCESS",
        row: 5,
        width: 4,
        hints: &[(52, "*RUN, *VLD, *SYN")],
    },
    AttrRow {
        key: "OUTPUT",
        row: 6,
        width: 1,
        hints: &[(52, "1=Display, 2=Printer"), (52, "3=File")],
    },
    AttrRow {
        key: "COMMIT",
        row: 8,
        width: 5,
        hints: &[(52, "*NONE, *CHG, *CS, *ALL, *RR"), (52, "*NC, *UR, *RS")],
    },
    AttrRow {
        key: "DATFMT",
        row: 10,
        width: 4,
        hints: &[
            (52, "*JOB, *USA, *ISO, *EUR, *JIS"),
            (52, "*MDY, *DMY, *YMD, *JUL"),
        ],
    },
    AttrRow {
        key: "DATSEP",
        row: 12,
        width: 6,
        hints: &[(52, "*JOB, '/', '.', ',', '-'"), (52, "' ', *BLANK")],
    },
    AttrRow {
        key: "TIMFMT",
        row: 14,
        width: 4,
        hints: &[(52, "*HMS, *USA, *ISO"), (52, "*EUR, *JIS")],
    },
    AttrRow {
        key: "TIMSEP",
        row: 16,
        width: 6,
        hints: &[(52, "*JOB, ':', '.', ','"), (52, "' ', *BLANK")],
    },
    AttrRow {
        key: "REFRESH",
        row: 18,
        width: 8,
        hints: &[(52, "*ALWAYS, *FORWARD")],
    },
    AttrRow {
        key: "ALWCPYDTA",
        row: 19,
        width: 9,
        hints: &[(52, "*YES, *OPTIMIZE, *NO")],
    },
    AttrRow {
        key: "NAMING",
        row: 20,
        width: 4,
        hints: &[(52, "*SYS, *SQL")],
    },
];

const PAGE2: &[AttrRow] = &[
    AttrRow {
        key: "LIBOPT",
        row: 5,
        width: 10,
        hints: &[
            (52, "Name, *LIBL, *USRLIBL"),
            (52, "*ALLUSR, *ALL, *CURLIB"),
        ],
    },
    AttrRow {
        key: "LISTTYPE",
        row: 7,
        width: 4,
        hints: &[(52, "*ALL, *SQL")],
    },
    AttrRow {
        key: "DECPNT",
        row: 8,
        width: 7,
        hints: &[(52, "*PERIOD, *COMMA, *JOB,"), (54, "*SYSVAL")],
    },
    AttrRow {
        key: "SRTSEQ",
        row: 10,
        width: 10,
        hints: &[
            (52, "Name, *JOB, *JOBRUN"),
            (52, "*LANGIDUNQ, *LANGIDSHR"),
            (52, "*HEX"),
        ],
    },
    AttrRow {
        key: "LANGID",
        row: 13,
        width: 7,
        hints: &[(52, "Name, *JOB, *JOBRUN"), (52, "F4 for Prompt")],
    },
    AttrRow {
        key: "SQLCURRULE",
        row: 15,
        width: 4,
        hints: &[(52, "*DB2, *STD")],
    },
    AttrRow {
        key: "DSPPWD",
        row: 16,
        width: 4,
        hints: &[(52, "*NO, *YES")],
    },
];

/// The pages of Change Session Attributes.
pub const ATTR_PAGES: usize = 2;

/// The attribute keys each page shows, for reading the fields back.
#[must_use]
pub fn attr_keys(page: usize) -> Vec<&'static str> {
    let rows = if page == 0 { PAGE1 } else { PAGE2 };
    rows.iter().map(|r| r.key).collect()
}

/// Change Session Attributes, page `page` from 0, with `values` (as
/// typed when redisplayed after an error).
#[must_use]
pub fn attributes(page: usize, a: &Attrs, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 28, "Change Session Attributes");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    let rows = if page == 0 { PAGE1 } else { PAGE2 };
    for r in rows {
        let label = crate::attrs::field(r.key).map_or("", |f| f.label);
        put(&mut s, r.row, 4, Attr::Normal, &leader(label, 4, 33));
        var(&mut s, r.row, 37, r.width, r.key, &a.shown(r.key));
        for (i, (col, h)) in r.hints.iter().enumerate() {
            put(&mut s, r.row + i, *col, Attr::Normal, h);
        }
    }
    if page == 0 {
        more(&mut s, 22, 79, false);
    } else {
        more(&mut s, 22, 79, true);
    }
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
    s.cursor = Some((rows[0].row, 37));
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
