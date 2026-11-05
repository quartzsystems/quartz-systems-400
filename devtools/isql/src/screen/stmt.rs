// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statement prompts of Select SQL Statement other than the seven
//! of [`super::prompt`]: ALTER TABLE to SET TRANSACTION, each placed as
//! the pub400 captures place it (`golden/sql/strsql/`). Their words
//! are `QSTXTMSG` identifiers (`TXT0Annn`), read through the job's
//! [`Words`]; IBM's keywords stay as written.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title, var};
use crate::assemble::{Kind, Values};
use crate::print::Words;

/// Text under each naming: an identifier (`TXT...`) or keywords.
#[derive(Clone, Copy, Debug)]
pub struct Label {
    /// Under `*SYS`.
    pub sys: &'static str,
    /// Under `*SQL`.
    pub sql: &'static str,
}

const fn t(s: &'static str) -> Label {
    Label { sys: s, sql: s }
}

const fn n(sys: &'static str, sql: &'static str) -> Label {
    Label { sys, sql }
}

/// The text of `l` under the naming: an identifier read through `w`.
fn say(l: &Label, sql: bool, w: &Words<'_>) -> String {
    let s = if sql { l.sql } else { l.sys };
    if s.len() == 7 && s.starts_with("TXT") { w(s) } else { s.to_string() }
}

/// One item of a prompt.
#[derive(Clone, Copy, Debug)]
pub enum Item {
    /// Fixed text.
    Text {
        /// Row.
        row: usize,
        /// Column.
        col: usize,
        /// Attribute.
        attr: Attr,
        /// The text.
        text: Label,
    },
    /// A label with leaders to `end`, `lines` fields under one another
    /// (named `NAME`, or `NAME1` .. `NAMEn` for more than one), the
    /// choices to the right a row each.
    Entry {
        /// Row.
        row: usize,
        /// The label's column.
        col: usize,
        /// The label.
        label: Label,
        /// The last leader column.
        end: usize,
        /// The field's column.
        field: usize,
        /// Its width.
        width: usize,
        /// How many fields.
        lines: usize,
        /// Its name.
        name: &'static str,
        /// Its value when the prompt opens empty.
        default: &'static str,
        /// The choices: column and text.
        hints: &'static [(usize, Label)],
    },
    /// CALL's numbered value lines, a page of `count` from `row`.
    Numbered {
        /// The first row.
        row: usize,
        /// Lines on a page.
        count: usize,
        /// Lines in all.
        total: usize,
        /// The column the numbers end on.
        number: usize,
        /// The field's column.
        field: usize,
        /// Its width.
        width: usize,
        /// The fields' name: `NAME1` .. `NAMEn`.
        name: &'static str,
    },
}

/// A statement's prompt.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    /// The statement's words, for the title.
    pub words: &'static str,
    /// The items.
    pub items: &'static [Item],
    /// The function keys: row, column, text.
    pub keys: &'static [(usize, usize, Label)],
    /// The row of a `Bottom` or `More...` indicator.
    pub more: Option<usize>,
}

/// The `QSTXTMSG` identifiers.
mod id {
    pub const TITLE: &str = "TXT0A01";
    pub const INFO: &str = "TXT0A02";
    pub const CHOICES: &str = "TXT0A03";
    pub const CHOICE: &str = "TXT0A04";
    pub const PER_LINE: &str = "TXT0A05";
    pub const FILE: &str = "TXT0A09";
    pub const TABLE: &str = "TXT0A0A";
    pub const LIBRARY: &str = "TXT0A0B";
    pub const NAME_F4: &str = "TXT0A0D";
    pub const NAME: &str = "TXT0A0E";
    pub const YES_NO: &str = "TXT0A25";
    pub const CONNECTION: &str = "TXT0A26";
    pub const RELATIONAL: &str = "TXT0A32";
    pub const DATABASE: &str = "TXT0A33";
    pub const DROP_OPTION: &str = "TXT0A35";
    pub const USER: &str = "TXT0A38";
    pub const PUBLIC: &str = "TXT0A39";
    pub const F3: &str = "TXT0A45";
    pub const F4: &str = "TXT0A46";
    pub const F5: &str = "TXT0A47";
    pub const F6: &str = "TXT0A48";
    pub const F10: &str = "TXT0A49";
    pub const F12: &str = "TXT0A4B";
    pub const F14: &str = "TXT0A4C";
    pub const F15: &str = "TXT0A4D";
    pub const F20: &str = "TXT0A50";
    pub const F21: &str = "TXT0A51";
    pub const F24: &str = "TXT0A52";
}

/// `File` or `Table` by the naming.
pub const FILE: Label = n(id::FILE, id::TABLE);
const LIBRARY: Label = t(id::LIBRARY);

const fn text(row: usize, s: &'static str) -> Item {
    Item::Text { row, col: 2, attr: Attr::Blue, text: t(s) }
}

#[allow(clippy::too_many_arguments)] // the columns of one entry
const fn entry(
    row: usize,
    col: usize,
    label: Label,
    end: usize,
    field: usize,
    width: usize,
    name: &'static str,
    default: &'static str,
    hints: &'static [(usize, Label)],
) -> Item {
    Item::Entry { row, col, label, end, field, width, lines: 1, name, default, hints }
}

const NAMES_KEYS: &[(usize, usize, Label)] = &[
    (22, 2, t(id::F3)),
    (22, 12, t(id::F4)),
    (22, 24, t(id::F5)),
    (22, 37, t(id::F12)),
    (22, 50, t(id::F20)),
    (23, 2, t(id::F21)),
];

const SHORT_KEYS: &[(usize, usize, Label)] =
    &[(23, 2, t(id::F3)), (23, 12, t(id::F5)), (23, 25, t(id::F12)), (23, 38, t(id::F21))];

const SHORT_KEYS_22: &[(usize, usize, Label)] =
    &[(22, 2, t(id::F3)), (22, 12, t(id::F5)), (22, 25, t(id::F12)), (22, 38, t(id::F21))];

const PROMPT_KEYS_23: &[(usize, usize, Label)] = &[
    (23, 2, t(id::F3)),
    (23, 12, t(id::F4)),
    (23, 24, t(id::F5)),
    (23, 37, t(id::F12)),
    (23, 50, t(id::F21)),
];

const GRANT_KEYS: &[(usize, usize, Label)] = &[
    (22, 2, t(id::F3)),
    (22, 15, t(id::F4)),
    (22, 33, t(id::F5)),
    (22, 46, t(id::F6)),
    (22, 63, t(id::F10)),
    (23, 2, t(id::F12)),
    (23, 15, t(id::F14)),
    (23, 33, t(id::F21)),
];

const GRANT_TABLE_KEYS: &[(usize, usize, Label)] = &[
    (22, 2, t(id::F3)),
    (22, 15, t(id::F4)),
    (22, 33, t(id::F5)),
    (22, 46, t(id::F6)),
    (22, 63, t(id::F10)),
    (23, 2, t(id::F12)),
    (23, 15, t(id::F14)),
    (23, 33, t(id::F20)),
    (23, 63, t(id::F24)),
];

/// Specify ALTER TABLE Statement.
pub const ALTER_TABLE: Panel = Panel {
    words: "ALTER TABLE",
    items: &[
        text(3, id::INFO),
        entry(5, 4, FILE, 23, 27, 20, "TABLE", "", &[(52, t(id::NAME_F4))]),
        entry(6, 6, LIBRARY, 23, 29, 12, "LIB", "", &[(52, t(id::NAME_F4))]),
        entry(
            8,
            4,
            t("TXT0A0C"),
            23,
            27,
            1,
            "ACTION",
            "",
            &[
                (52, t("TXT0A0F")),
                (52, t("TXT0A10")),
                (52, n("TXT0A11", "TXT0A12")),
                (52, n("TXT0A13", "TXT0A14")),
                (52, n("TXT0A15", "TXT0A16")),
            ],
        ),
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify CALL Statement.
pub const CALL: Panel = Panel {
    words: "CALL",
    items: &[
        text(3, id::INFO),
        entry(5, 4, t("TXT0A17"), 36, 40, 18, "PROC", "", &[(65, t(id::NAME))]),
        entry(6, 6, LIBRARY, 36, 42, 12, "LIB", "", &[(65, t(id::NAME))]),
        Item::Text { row: 8, col: 2, attr: Attr::HighIntensity, text: t("TXT0A18") },
        Item::Text { row: 8, col: 16, attr: Attr::HighIntensity, text: t("TXT0A19") },
        Item::Numbered { row: 9, count: 12, total: CALL_PARMS, number: 7, field: 16, width: 53, name: "PARM" },
    ],
    keys: &[
        (22, 2, t(id::F3)),
        (22, 20, t(id::F5)),
        (22, 33, t(id::F6)),
        (22, 50, t(id::F10)),
        (22, 66, t(id::F12)),
        (23, 2, t(id::F14)),
        (23, 20, t(id::F15)),
        (23, 37, t(id::F20)),
        (23, 66, t(id::F24)),
    ],
    more: Some(21),
};

/// The parameters CALL's prompt takes.
pub const CALL_PARMS: usize = 48;

/// Specify COMMENT ON Statement.
pub const COMMENT_ON: Panel = Panel {
    words: "COMMENT ON",
    items: &[
        text(3, id::CHOICES),
        entry(
            5,
            4,
            t("TXT0A1A"),
            27,
            31,
            1,
            "OBJ",
            "1",
            &[
                (50, n("TXT0A1B", "TXT0A1C")),
                (50, n("TXT0A1D", "TXT0A1E")),
                (50, t("TXT0A1F")),
                (50, t("TXT0A20")),
                (50, t("TXT0A21")),
                (50, t("TXT0A22")),
                (50, t("TXT0A23")),
            ],
        ),
    ],
    keys: SHORT_KEYS_22,
    more: None,
};

const HOLD: &[Item] = &[text(3, id::CHOICE), entry(5, 4, t("TXT0A24"), 35, 39, 1, "HOLD", "N", &[(43, t(id::YES_NO))])];

/// Specify COMMIT Statement.
pub const COMMIT: Panel = Panel { words: "COMMIT", items: HOLD, keys: SHORT_KEYS, more: None };

/// Specify ROLLBACK Statement.
pub const ROLLBACK: Panel = Panel { words: "ROLLBACK", items: HOLD, keys: SHORT_KEYS, more: None };

/// Specify CONNECT Statement.
pub const CONNECT: Panel = Panel {
    words: "CONNECT",
    items: &[
        text(3, id::CHOICES),
        entry(
            5,
            4,
            t(id::CONNECTION),
            35,
            39,
            1,
            "CONN",
            "3",
            &[(62, t("TXT0A27")), (62, t("TXT0A28")), (62, t("TXT0A29"))],
        ),
    ],
    keys: SHORT_KEYS,
    more: None,
};

const WHICH_CONNECTION: &[Item] = &[
    text(3, id::CHOICES),
    entry(
        5,
        4,
        t(id::CONNECTION),
        35,
        39,
        1,
        "CONN",
        "",
        &[(62, t(id::RELATIONAL)), (64, t(id::DATABASE)), (62, t("2=CURRENT")), (62, t("3=ALL"))],
    ),
];

/// Specify DISCONNECT Statement.
pub const DISCONNECT: Panel = Panel { words: "DISCONNECT", items: WHICH_CONNECTION, keys: SHORT_KEYS, more: None };

/// Specify RELEASE Statement.
pub const RELEASE: Panel = Panel { words: "RELEASE", items: WHICH_CONNECTION, keys: SHORT_KEYS, more: None };

/// Specify CREATE ALIAS Statement.
pub const CREATE_ALIAS: Panel = Panel {
    words: "CREATE ALIAS",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A2A"), 27, 31, 20, "ALIAS", "", &[(56, t(id::NAME))]),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", &[(56, t(id::NAME_F4))]),
        entry(8, 4, FILE, 27, 31, 20, "TABLE", "", &[(56, t(id::NAME_F4))]),
        entry(9, 6, LIBRARY, 27, 33, 12, "TABLELIB", "", &[(56, t(id::NAME_F4))]),
        entry(10, 4, t("TXT0A2B"), 27, 31, 12, "MEMBER", "", &[(56, t("TXT0A2C")), (56, t("TXT0A2D"))]),
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify CREATE COLLECTION Statement.
pub const CREATE_COLLECTION: Panel = Panel {
    words: "CREATE COLLECTION",
    items: &[
        text(3, id::CHOICE),
        entry(5, 4, LIBRARY, 35, 39, 12, "LIB", "", &[(54, t(id::NAME))]),
        entry(7, 4, t("IN ASP"), 35, 39, 12, "ASP", "", &[(54, t("TXT0A2E"))]),
        entry(8, 4, t("WITH DATA DICTIONARY"), 35, 39, 1, "DICT", "N", &[(54, t(id::YES_NO))]),
    ],
    keys: SHORT_KEYS,
    more: None,
};

/// Specify CREATE PROCEDURE Statement.
pub const CREATE_PROCEDURE: Panel = Panel {
    words: "CREATE PROCEDURE",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A17"), 25, 29, 18, "PROC", "", &[(52, t(id::NAME))]),
        entry(6, 6, LIBRARY, 25, 31, 12, "LIB", "", &[(52, t(id::NAME_F4))]),
        entry(8, 4, t("RESULT SETS"), 25, 29, 5, "RESULT", "", &[(52, t("TXT0A2F"))]),
        entry(
            10,
            4,
            t("TXT0A30"),
            25,
            29,
            7,
            "LANG",
            "",
            &[(52, t("C, CL, COBOL, COBOLLE,")), (52, t("C++, FORTRAN, PLI,")), (52, t("REXX, RPG, RPGLE,")), (52, t("TXT0A31"))],
        ),
        entry(
            15,
            4,
            t("PARAMETER STYLE"),
            25,
            29,
            1,
            "STYLE",
            "1",
            &[
                (52, t("1=GENERAL")),
                (52, t("2=GENERAL WITH NULLS")),
                (52, t("3=SQL")),
                (52, t("4=JAVA")),
                (52, t("5=DB2GENERAL")),
            ],
        ),
    ],
    keys: NAMES_KEYS,
    more: None,
};

const NAME_F4_56: &[(usize, Label)] = &[(56, t(id::NAME_F4))];
const DROP_OPTION: Item = entry(8, 4, t(id::DROP_OPTION), 27, 31, 1, "OPT", "1", &[(56, t("1=CASCADE, 2=RESTRICT"))]);

/// Specify DROP ALIAS Statement.
pub const DROP_ALIAS: Panel = Panel {
    words: "DROP ALIAS",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A2A"), 27, 31, 20, "ALIAS", "", NAME_F4_56),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", NAME_F4_56),
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify DROP INDEX Statement.
pub const DROP_INDEX: Panel = Panel {
    words: "DROP INDEX",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A34"), 27, 31, 20, "INDEX", "", NAME_F4_56),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", NAME_F4_56),
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify DROP TABLE Statement.
pub const DROP_TABLE: Panel = Panel {
    words: "DROP TABLE",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, FILE, 27, 31, 20, "TABLE", "", NAME_F4_56),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", NAME_F4_56),
        DROP_OPTION,
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify DROP VIEW Statement.
pub const DROP_VIEW: Panel = Panel {
    words: "DROP VIEW",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A37"), 27, 31, 20, "VIEW", "", NAME_F4_56),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", NAME_F4_56),
        DROP_OPTION,
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify DROP COLLECTION Statement.
pub const DROP_COLLECTION: Panel = Panel {
    words: "DROP COLLECTION",
    items: &[
        text(3, id::CHOICE),
        entry(5, 4, LIBRARY, 35, 39, 12, "LIB", "", &[(56, t(id::NAME_F4))]),
        entry(7, 4, t(id::DROP_OPTION), 35, 39, 1, "OPT", "1", &[(56, t("1=CASCADE, 2=RESTRICT"))]),
    ],
    keys: PROMPT_KEYS_23,
    more: None,
};

/// Specify DROP PACKAGE Statement.
pub const DROP_PACKAGE: Panel = Panel {
    words: "DROP PACKAGE",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A36"), 35, 39, 12, "PKG", "", &[(56, t(id::NAME_F4))]),
        entry(6, 6, LIBRARY, 35, 41, 12, "LIB", "", &[(56, t(id::NAME_F4))]),
    ],
    keys: PROMPT_KEYS_23,
    more: None,
};

/// GRANT's and REVOKE's names: three lines of objects, three of users.
#[allow(clippy::too_many_arguments)] // the columns of one entry
const fn lines3(row: usize, label: Label, end: usize, field: usize, width: usize, name: &'static str, hints: &'static [(usize, Label)]) -> Item {
    Item::Entry { row, col: 4, label, end, field, width, lines: 3, name, default: "", hints }
}

const PACKAGE_NAMES: &[Item] = &[
    text(3, id::PER_LINE),
    lines3(5, t("TXT0A36"), 30, 34, 25, "PKG", &[(62, t(id::NAME_F4))]),
    lines3(8, t(id::USER), 30, 34, 25, "USER", &[(62, t(id::PUBLIC))]),
];

const TABLE_NAMES: &[Item] = &[
    text(3, id::PER_LINE),
    lines3(5, FILE, 20, 23, 33, "TABLE", &[(61, t(id::NAME_F4))]),
    lines3(8, t(id::USER), 20, 23, 33, "USER", &[(61, t(id::PUBLIC))]),
];

/// Specify GRANT PACKAGE Statement.
pub const GRANT_PACKAGE: Panel =
    Panel { words: "GRANT PACKAGE", items: PACKAGE_NAMES, keys: GRANT_KEYS, more: Some(21) };

/// Specify REVOKE PACKAGE Statement.
pub const REVOKE_PACKAGE: Panel =
    Panel { words: "REVOKE PACKAGE", items: PACKAGE_NAMES, keys: GRANT_KEYS, more: Some(21) };

/// Specify GRANT TABLE Statement.
pub const GRANT_TABLE: Panel =
    Panel { words: "GRANT TABLE", items: TABLE_NAMES, keys: GRANT_TABLE_KEYS, more: Some(21) };

/// Specify REVOKE TABLE Statement.
pub const REVOKE_TABLE: Panel =
    Panel { words: "REVOKE TABLE", items: TABLE_NAMES, keys: GRANT_TABLE_KEYS, more: Some(21) };

/// Specify LABEL ON Statement.
pub const LABEL_ON: Panel = Panel {
    words: "LABEL ON",
    items: &[
        text(3, id::CHOICES),
        entry(
            5,
            4,
            t("TXT0A3A"),
            19,
            23,
            1,
            "OBJ",
            "1",
            &[(50, n("TXT0A1B", "TXT0A1C")), (50, n("TXT0A1D", "TXT0A1E")), (50, t("TXT0A1F")), (50, t("TXT0A3B"))],
        ),
    ],
    keys: SHORT_KEYS_22,
    more: None,
};

/// Specify LOCK TABLE Statement.
pub const LOCK_TABLE: Panel = Panel {
    words: "LOCK TABLE",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, FILE, 27, 31, 20, "TABLE", "", &[(56, t(id::NAME_F4))]),
        entry(6, 6, LIBRARY, 27, 33, 12, "LIB", "", &[(56, t(id::NAME_F4))]),
        entry(8, 4, t("TXT0A3C"), 27, 31, 1, "MODE", "1", &[(56, t("1=SHARE")), (56, t("2=EXCLUSIVE"))]),
    ],
    keys: NAMES_KEYS,
    more: None,
};

/// Specify RENAME Statement.
pub const RENAME: Panel = Panel {
    words: "RENAME",
    items: &[
        text(3, id::CHOICES),
        entry(5, 4, t("TXT0A3D"), 23, 27, 1, "OBJ", "1", &[(52, n("TXT0A1B", "TXT0A1C")), (52, t("TXT0A3E"))]),
    ],
    keys: SHORT_KEYS,
    more: None,
};

/// Specify SET CONNECTION Statement.
pub const SET_CONNECTION: Panel = Panel {
    words: "SET CONNECTION",
    items: &[text(3, id::CHOICES), entry(5, 4, t("TXT0A3F"), 35, 39, 20, "RDB", "", &[(62, t(id::NAME_F4))])],
    keys: PROMPT_KEYS_23,
    more: None,
};

/// Specify SET TRANSACTION Statement.
pub const SET_TRANSACTION: Panel = Panel {
    words: "SET TRANSACTION",
    items: &[
        text(3, id::CHOICES),
        entry(
            5,
            4,
            t("ISOLATION LEVEL"),
            34,
            38,
            1,
            "ISOL",
            "",
            &[
                (49, t("1=NO COMMIT (NC, NONE)")),
                (49, t("2=READ UNCOMMITTED (UR, CHG)")),
                (49, t("3=READ COMMITTED (CS)")),
                (49, t("4=REPEATABLE READ (RS, ALL)")),
                (49, t("5=SERIALIZABLE (RR)")),
            ],
        ),
    ],
    keys: SHORT_KEYS,
    more: None,
};

/// The prompt of a statement this module has, `None` for the seven of
/// [`super::prompt`].
#[must_use]
pub fn of(kind: Kind) -> Option<&'static Panel> {
    Some(match kind {
        Kind::AlterTable => &ALTER_TABLE,
        Kind::Call => &CALL,
        Kind::CommentOn => &COMMENT_ON,
        Kind::Commit => &COMMIT,
        Kind::Connect => &CONNECT,
        Kind::CreateAlias => &CREATE_ALIAS,
        Kind::CreateCollection => &CREATE_COLLECTION,
        Kind::CreateProcedure => &CREATE_PROCEDURE,
        Kind::Disconnect => &DISCONNECT,
        Kind::DropAlias => &DROP_ALIAS,
        Kind::DropCollection => &DROP_COLLECTION,
        Kind::DropIndex => &DROP_INDEX,
        Kind::DropPackage => &DROP_PACKAGE,
        Kind::DropTable => &DROP_TABLE,
        Kind::DropView => &DROP_VIEW,
        Kind::GrantPackage => &GRANT_PACKAGE,
        Kind::GrantTable => &GRANT_TABLE,
        Kind::LabelOn => &LABEL_ON,
        Kind::LockTable => &LOCK_TABLE,
        Kind::Rename => &RENAME,
        Kind::Release => &RELEASE,
        Kind::RevokePackage => &REVOKE_PACKAGE,
        Kind::RevokeTable => &REVOKE_TABLE,
        Kind::Rollback => &ROLLBACK,
        Kind::SetConnection => &SET_CONNECTION,
        Kind::SetTransaction => &SET_TRANSACTION,
        _ => return None,
    })
}

/// A field's name: `NAME` alone, `NAME1` .. for several.
#[must_use]
pub fn field_name(name: &str, lines: usize, i: usize) -> String {
    if lines == 1 { name.to_string() } else { format!("{name}{}", i + 1) }
}

/// The pages a prompt has (CALL's parameters run over several).
#[must_use]
pub fn pages(p: &Panel) -> usize {
    p.items
        .iter()
        .find_map(|i| match i {
            Item::Numbered { count, total, .. } => Some(total.div_ceil(*count)),
            _ => None,
        })
        .unwrap_or(1)
}

/// The names of the fields page `page` shows.
#[must_use]
pub fn names(p: &Panel, page: usize) -> Vec<String> {
    let mut out = Vec::new();
    for item in p.items {
        match item {
            Item::Entry { lines, name, .. } => out.extend((0..*lines).map(|i| field_name(name, *lines, i))),
            Item::Numbered { count, total, name, .. } => {
                let first = page * count;
                out.extend((first..(first + count).min(*total)).map(|i| format!("{name}{}", i + 1)));
            }
            Item::Text { .. } => {}
        }
    }
    out
}

/// Every entry's default where nothing is typed.
#[must_use]
pub fn defaults(p: &Panel, typed: &Values) -> Values {
    let mut v = typed.clone();
    for item in p.items {
        if let Item::Entry { name, default, lines: 1, .. } = item
            && !default.is_empty()
            && v.get(*name).is_none_or(|s| s.trim().is_empty())
        {
            v.insert((*name).to_string(), (*default).to_string());
        }
    }
    v
}

/// The prompt, page `page`, with `values`.
#[must_use]
#[allow(clippy::many_single_char_names)] // the panel, its screen, the words
pub fn render(p: &Panel, values: &Values, page: usize, sql: bool, msg: Option<&str>, w: &Words<'_>) -> Screen {
    let get = |n: &str, d: &str| values.get(n).cloned().unwrap_or_else(|| d.to_string());
    let mut s = blank();
    let heading = w(id::TITLE).replace("&1", p.words);
    title(&mut s, (80 - heading.chars().count().min(80)) / 2 + 1, &heading);
    let mut first = None;
    let mut last_page = true;
    for item in p.items {
        match item {
            Item::Text { row, col, attr, text } => put(&mut s, *row, *col, *attr, &say(text, sql, w)),
            Item::Entry { row, col, label, end, field, width, lines, name, default, hints } => {
                put(&mut s, *row, *col, Attr::Normal, &leader(&say(label, sql, w), *col, *end));
                for i in 0..*lines {
                    let f = field_name(name, *lines, i);
                    let d = if i == 0 { *default } else { "" };
                    var(&mut s, row + i, *field, *width, &f, &get(&f, d));
                }
                for (i, (c, h)) in hints.iter().enumerate() {
                    put(&mut s, row + i, *c, Attr::Normal, &say(h, sql, w));
                }
                first.get_or_insert((*row, *field));
            }
            Item::Numbered { row, count, total, number, field, width, name } => {
                let start = page * count;
                for (k, i) in (start..(start + count).min(*total)).enumerate() {
                    let num = (i + 1).to_string();
                    put(&mut s, row + k, number + 1 - num.len(), Attr::Normal, &num);
                    let f = format!("{name}{}", i + 1);
                    var(&mut s, row + k, *field, *width, &f, &get(&f, ""));
                }
                last_page = start + count >= *total;
            }
        }
    }
    if let Some(r) = p.more {
        more(&mut s, r, 79, last_page);
    }
    let shown: Vec<(usize, usize, String)> = p.keys.iter().map(|(r, c, l)| (*r, *c, say(l, sql, w))).collect();
    let refs: Vec<(usize, usize, &str)> = shown.iter().map(|(r, c, l)| (*r, *c, l.as_str())).collect();
    keys(&mut s, &refs);
    message(&mut s, msg);
    s.cursor = first;
    s
}
