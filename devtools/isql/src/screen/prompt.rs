// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statement prompts F4 shows: Specify SELECT, INSERT, UPDATE,
//! DELETE, CREATE TABLE, CREATE VIEW and CREATE INDEX Statement, each
//! a table of items placed as the goldens place them, and the second
//! displays some of them lead to (the values to insert, the columns to
//! set, the index's key). Field names are the ones
//! [`crate::assemble`] reads.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title, var};
use crate::assemble::{Kind, Values};

/// A label under each naming: `*SYS` (files, fields) and `*SQL`
/// (tables, columns).
#[derive(Clone, Copy, Debug)]
pub struct Label {
    /// Under `*SYS`.
    pub sys: &'static str,
    /// Under `*SQL`.
    pub sql: &'static str,
}

const fn same(s: &'static str) -> Label {
    Label { sys: s, sql: s }
}

const fn named(sys: &'static str, sql: &'static str) -> Label {
    Label { sys, sql }
}

/// One item of a prompt.
#[derive(Clone, Copy, Debug)]
pub enum Item {
    /// A label with leaders to `end`, its field, the choices to its
    /// right (`(column, text)` a row each).
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
        /// Its name.
        name: &'static str,
        /// Its value when the prompt opens empty.
        default: &'static str,
        /// The choices.
        hints: &'static [(usize, &'static str)],
    },
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
    /// Input lines named `NAME1` .. `NAMEn`, read back as one value.
    Lines {
        /// The first row.
        row: usize,
        /// Column.
        col: usize,
        /// Width.
        width: usize,
        /// How many.
        count: usize,
        /// The name.
        name: &'static str,
    },
    /// CREATE TABLE's column table: `count` rows of name, `FOR` name,
    /// type, length, scale and nulls.
    Columns {
        /// The first row.
        row: usize,
        /// How many.
        count: usize,
    },
}

/// A prompt display.
#[derive(Clone, Copy, Debug)]
pub struct Prompt {
    /// The title's column.
    pub title_col: usize,
    /// The title.
    pub title: &'static str,
    /// The items.
    pub items: &'static [Item],
    /// The function keys.
    pub keys: &'static [(usize, usize, &'static str)],
    /// The row of a `Bottom` indicator, when the display has one.
    pub bottom: Option<usize>,
}

const TYPE_CHOICES: Item = Item::Text {
    row: 3,
    col: 2,
    attr: Attr::Blue,
    text: same("Type choices, press Enter."),
};

const ISOLATION: &[(usize, &str)] = &[
    (50, "1=Current level, 2=NC (NONE)"),
    (50, "3=UR (CHG), 4=CS, 5=RS (ALL)"),
    (50, "6=RR"),
];

const LIST_KEYS: &[(usize, usize, &str)] = &[
    (22, 2, "F3=Exit"),
    (22, 18, "F4=Prompt"),
    (22, 30, "F5=Refresh"),
    (22, 43, "F6=Insert line"),
    (22, 60, "F9=Specify subquery"),
    (23, 2, "F10=Copy line"),
    (23, 18, "F12=Cancel"),
    (23, 31, "F14=Delete line"),
    (23, 49, "F15=Split line"),
    (23, 66, "F24=More keys"),
];

const NAME_KEYS: &[(usize, usize, &str)] = &[
    (22, 2, "F3=Exit"),
    (22, 12, "F4=Prompt"),
    (22, 24, "F5=Refresh"),
    (22, 37, "F12=Cancel"),
    (22, 50, "F20=Display entire name"),
    (23, 2, "F21=Display statement"),
];

const SHORT_KEYS: &[(usize, usize, &str)] = &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")];

const YES_NO: &[(usize, &str)] = &[(60, "Y=Yes, N=No")];

const fn sel(row: usize, label: Label, end: usize, name: &'static str) -> Item {
    Item::Entry {
        row,
        col: 4,
        label,
        end,
        field: 35,
        width: 45,
        name,
        default: "",
        hints: &[],
    }
}

const fn yn(row: usize, label: &'static str, name: &'static str) -> Item {
    Item::Entry {
        row,
        col: 4,
        label: same(label),
        end: 51,
        field: 55,
        width: 1,
        name,
        default: "N",
        hints: YES_NO,
    }
}

/// Specify SELECT Statement.
pub const SELECT: Prompt = Prompt {
    title_col: 29,
    title: "Specify SELECT Statement",
    items: &[
        Item::Text {
            row: 3,
            col: 2,
            attr: Attr::Blue,
            text: same("Type SELECT statement information.  Press F4 for a list."),
        },
        sel(5, named("FROM files", "FROM tables"), 31, "FROM"),
        sel(6, named("SELECT fields", "SELECT columns"), 31, "SELECT"),
        sel(7, same("WHERE conditions"), 31, "WHERE"),
        sel(
            8,
            named("GROUP BY fields", "GROUP BY columns"),
            31,
            "GROUPBY",
        ),
        sel(9, same("HAVING conditions"), 31, "HAVING"),
        sel(
            10,
            named("ORDER BY fields", "ORDER BY columns"),
            31,
            "ORDERBY",
        ),
        sel(
            11,
            named("FOR UPDATE OF fields", "FOR UPDATE OF columns"),
            31,
            "FORUPDATE",
        ),
        Item::Text {
            row: 14,
            col: 2,
            attr: Attr::Blue,
            text: same("Type choices, press Enter."),
        },
        yn(16, "DISTINCT records in result file", "DISTINCT"),
        yn(17, "UNION with another SELECT", "UNION"),
        yn(18, "Specify additional options", "OPTIONS"),
    ],
    keys: LIST_KEYS,
    bottom: Some(13),
};

#[allow(clippy::too_many_arguments)] // the columns of one entry
const fn name_entry(
    row: usize,
    col: usize,
    label: Label,
    end: usize,
    field: usize,
    width: usize,
    name: &'static str,
    hint: &'static [(usize, &'static str)],
) -> Item {
    Item::Entry {
        row,
        col,
        label,
        end,
        field,
        width,
        name,
        default: "",
        hints: hint,
    }
}

/// Specify INSERT Statement.
pub const INSERT: Prompt = Prompt {
    title_col: 29,
    title: "Specify INSERT Statement",
    items: &[
        TYPE_CHOICES,
        name_entry(
            5,
            4,
            named("INTO file", "INTO table"),
            28,
            32,
            20,
            "INTO",
            &[(57, "Name, F4 for list")],
        ),
        name_entry(
            6,
            6,
            same("Library"),
            28,
            34,
            12,
            "LIB",
            &[(57, "Name, F4 for list")],
        ),
        Item::Text {
            row: 8,
            col: 4,
            attr: Attr::Normal,
            text: named("Select fields to insert", "Select columns to insert"),
        },
        Item::Entry {
            row: 9,
            col: 6,
            label: same("INTO"),
            end: 28,
            field: 32,
            width: 1,
            name: "COLSEL",
            default: "N",
            hints: &[(57, "Y=Yes, N=No")],
        },
        Item::Entry {
            row: 11,
            col: 4,
            label: same("Insertion method"),
            end: 28,
            field: 32,
            width: 1,
            name: "METHOD",
            default: "1",
            hints: &[(57, "1=Input VALUES"), (57, "2=Subselect")],
        },
        Item::Text {
            row: 14,
            col: 2,
            attr: Attr::Blue,
            text: same("Type choices, press Enter."),
        },
        Item::Entry {
            row: 16,
            col: 4,
            label: same("WITH isolation level"),
            end: 28,
            field: 32,
            width: 1,
            name: "ISOL",
            default: "1",
            hints: ISOLATION,
        },
    ],
    keys: NAME_KEYS,
    bottom: None,
};

/// Specify UPDATE Statement.
pub const UPDATE: Prompt = Prompt {
    title_col: 29,
    title: "Specify UPDATE Statement",
    items: &[
        TYPE_CHOICES,
        name_entry(
            5,
            4,
            named("File", "Table"),
            25,
            29,
            20,
            "TABLE",
            &[(54, "Name, F4 for list")],
        ),
        name_entry(
            6,
            6,
            same("Library"),
            25,
            31,
            12,
            "LIB",
            &[(54, "Name, F4 for list")],
        ),
        name_entry(
            8,
            4,
            same("Correlation"),
            25,
            29,
            20,
            "CORR",
            &[(54, "Name")],
        ),
    ],
    keys: NAME_KEYS,
    bottom: None,
};

/// Specify DELETE Statement.
pub const DELETE: Prompt = Prompt {
    title_col: 29,
    title: "Specify DELETE Statement",
    items: &[
        TYPE_CHOICES,
        name_entry(
            5,
            4,
            named("File", "Table"),
            29,
            33,
            20,
            "TABLE",
            &[(58, "Name, F4 for list")],
        ),
        name_entry(
            6,
            6,
            same("Library"),
            29,
            35,
            12,
            "LIB",
            &[(58, "Name, F4 for list")],
        ),
        name_entry(
            7,
            4,
            same("Correlation"),
            29,
            33,
            20,
            "CORR",
            &[(58, "Name")],
        ),
        Item::Text {
            row: 9,
            col: 2,
            attr: Attr::Blue,
            text: same("Type WHERE conditions, press Enter."),
        },
        Item::Lines {
            row: 10,
            col: 4,
            width: 73,
            count: 2,
            name: "WHERE",
        },
        Item::Text {
            row: 14,
            col: 2,
            attr: Attr::Blue,
            text: same("Type choices, press Enter."),
        },
        Item::Entry {
            row: 16,
            col: 4,
            label: same("WITH isolation level"),
            end: 29,
            field: 33,
            width: 1,
            name: "ISOL",
            default: "1",
            hints: ISOLATION,
        },
    ],
    keys: LIST_KEYS,
    bottom: Some(13),
};

/// Specify CREATE TABLE Statement.
pub const CREATE_TABLE: Prompt = Prompt {
    title_col: 26,
    title: "Specify CREATE TABLE Statement",
    items: &[
        Item::Text {
            row: 3,
            col: 2,
            attr: Attr::Blue,
            text: same("Type information, press Enter."),
        },
        name_entry(
            5,
            4,
            named("File", "Table"),
            27,
            31,
            20,
            "TABLE",
            &[(56, "Name")],
        ),
        name_entry(
            6,
            6,
            same("Library"),
            27,
            33,
            12,
            "LIB",
            &[(56, "Name, F4 for list")],
        ),
        Item::Text {
            row: 8,
            col: 2,
            attr: Attr::Blue,
            text: same("Nulls:  1=NULL, 2=NOT NULL, 3=NOT NULL WITH DEFAULT"),
        },
        Item::Columns { row: 11, count: 7 },
        Item::Entry {
            row: 19,
            col: 4,
            label: named("File CONSTRAINT", "Table CONSTRAINT"),
            end: 45,
            field: 49,
            width: 1,
            name: "CONSTRAINT",
            default: "N",
            hints: &[(56, "Y=Yes, N=No")],
        },
        Item::Entry {
            row: 20,
            col: 4,
            label: named("Distributed File", "Distributed Table"),
            end: 45,
            field: 49,
            width: 1,
            name: "DISTRIB",
            default: "N",
            hints: &[(56, "Y=Yes, N=No")],
        },
    ],
    keys: &[
        (22, 2, "F3=Exit"),
        (22, 12, "F4=Prompt"),
        (22, 32, "F5=Refresh"),
        (22, 45, "F6=Insert line"),
        (22, 63, "F10=Copy line"),
        (23, 2, "F11=Display more attributes"),
        (23, 32, "F12=Cancel"),
        (23, 45, "F14=Delete line"),
        (23, 63, "F24=More keys"),
    ],
    bottom: Some(18),
};

/// Specify CREATE VIEW Statement.
pub const CREATE_VIEW: Prompt = Prompt {
    title_col: 26,
    title: "Specify CREATE VIEW Statement",
    items: &[
        TYPE_CHOICES,
        name_entry(5, 4, same("View"), 25, 29, 20, "VIEW", &[(54, "Name")]),
        name_entry(
            6,
            6,
            same("Library"),
            25,
            31,
            12,
            "LIB",
            &[(54, "Name, F4 for list")],
        ),
        Item::Entry {
            row: 8,
            col: 4,
            label: named("Field names for view", "Column names for view"),
            end: 0,
            field: 29,
            width: 1,
            name: "NAMES",
            default: "1",
            hints: &[(54, "1=Use result file names"), (54, "2=Specify names")],
        },
        Item::Entry {
            row: 11,
            col: 4,
            label: same("WITH CHECK OPTION"),
            end: 25,
            field: 29,
            width: 1,
            name: "CHECK",
            default: "1",
            hints: &[(54, "1=NONE"), (54, "2=CASCADED"), (54, "3=LOCAL")],
        },
    ],
    keys: NAME_KEYS,
    bottom: None,
};

/// Specify CREATE INDEX Statement.
pub const CREATE_INDEX: Prompt = Prompt {
    title_col: 26,
    title: "Specify CREATE INDEX Statement",
    items: &[
        TYPE_CHOICES,
        name_entry(5, 4, same("Index"), 24, 28, 20, "INDEX", &[(53, "Name")]),
        name_entry(
            6,
            6,
            same("Library"),
            24,
            30,
            12,
            "LIB",
            &[(53, "Name, F4 for list")],
        ),
        name_entry(
            8,
            4,
            named("File", "Table"),
            24,
            28,
            20,
            "TABLE",
            &[(53, "Name, F4 for list")],
        ),
        name_entry(
            9,
            6,
            same("Library"),
            24,
            30,
            12,
            "TABLELIB",
            &[(53, "Name, F4 for list")],
        ),
        Item::Entry {
            row: 11,
            col: 4,
            label: same("Type of Index"),
            end: 24,
            field: 28,
            width: 1,
            name: "ITYPE",
            default: "1",
            hints: &[
                (53, "1=Unspecified"),
                (53, "2=UNIQUE"),
                (53, "3=UNIQUE WHERE NOT NULL"),
                (53, "4=ENCODED VECTOR"),
            ],
        },
        name_entry(
            16,
            4,
            same("DISTINCT VALUES"),
            24,
            28,
            10,
            "DISTVAL",
            &[(53, "Number")],
        ),
    ],
    keys: NAME_KEYS,
    bottom: None,
};

/// The second INSERT display: the columns and the values (or the
/// subselect).
pub const INSERT_VALUES: Prompt = Prompt {
    title_col: 29,
    title: "Specify INSERT Statement",
    items: &[
        Item::Text {
            row: 3,
            col: 2,
            attr: Attr::Blue,
            text: same("Type the values to insert, press Enter."),
        },
        Item::Text {
            row: 5,
            col: 2,
            attr: Attr::HighIntensity,
            text: named("Fields, in order", "Columns, in order"),
        },
        Item::Lines {
            row: 6,
            col: 4,
            width: 73,
            count: 3,
            name: "COLUMNS",
        },
        Item::Text {
            row: 10,
            col: 2,
            attr: Attr::HighIntensity,
            text: same("VALUES, or a subselect"),
        },
        Item::Lines {
            row: 11,
            col: 4,
            width: 73,
            count: 6,
            name: "VALUES",
        },
    ],
    keys: SHORT_KEYS,
    bottom: None,
};

/// The second UPDATE display: what to set and where.
pub const UPDATE_SET: Prompt = Prompt {
    title_col: 29,
    title: "Specify UPDATE Statement",
    items: &[
        Item::Text {
            row: 3,
            col: 2,
            attr: Attr::Blue,
            text: same("Type the changes as NAME = value, press Enter."),
        },
        Item::Text {
            row: 5,
            col: 2,
            attr: Attr::HighIntensity,
            text: same("SET"),
        },
        Item::Lines {
            row: 6,
            col: 4,
            width: 73,
            count: 5,
            name: "SET",
        },
        Item::Text {
            row: 12,
            col: 2,
            attr: Attr::HighIntensity,
            text: same("WHERE conditions"),
        },
        Item::Lines {
            row: 13,
            col: 4,
            width: 73,
            count: 4,
            name: "WHERE",
        },
    ],
    keys: SHORT_KEYS,
    bottom: None,
};

/// The second CREATE INDEX display: the key.
pub const INDEX_KEY: Prompt = Prompt {
    title_col: 26,
    title: "Specify CREATE INDEX Statement",
    items: &[
        Item::Text {
            row: 3,
            col: 2,
            attr: Attr::Blue,
            text: named(
                "Type the key fields, each with ASC or DESC, press Enter.",
                "Type the key columns, each with ASC or DESC, press Enter.",
            ),
        },
        Item::Lines {
            row: 5,
            col: 4,
            width: 73,
            count: 4,
            name: "COLUMNS",
        },
    ],
    keys: SHORT_KEYS,
    bottom: None,
};

/// The prompt a statement opens with.
#[must_use]
pub fn of(kind: Kind) -> &'static Prompt {
    match kind {
        Kind::Select => &SELECT,
        Kind::Insert => &INSERT,
        Kind::Update => &UPDATE,
        Kind::Delete => &DELETE,
        Kind::CreateTable => &CREATE_TABLE,
        Kind::CreateView => &CREATE_VIEW,
        Kind::CreateIndex => &CREATE_INDEX,
    }
}

/// The columns of CREATE TABLE's column table: name, column, width.
pub const COLUMN_FIELDS: &[(&str, usize, usize)] = &[
    ("FIELD", 2, 18),
    ("FOR", 24, 12),
    ("TYPE", 38, 14),
    ("LEN", 56, 10),
    ("SCALE", 69, 2),
    ("NULLS", 77, 1),
];

/// A column-table field's name: `FIELD3`, `NULLS0`.
#[must_use]
pub fn cell(name: &str, i: usize) -> String {
    format!("{name}{i}")
}

/// A line of a [`Item::Lines`] item: `WHERE1`.
#[must_use]
pub fn line(name: &str, i: usize) -> String {
    format!("{name}{}", i + 1)
}

/// The names of every field of a prompt.
#[must_use]
pub fn names(p: &Prompt) -> Vec<String> {
    let mut out = Vec::new();
    for item in p.items {
        match item {
            Item::Entry { name, .. } => out.push((*name).to_string()),
            Item::Lines { count, name, .. } => {
                out.extend((0..*count).map(|i| line(name, i)));
            }
            Item::Columns { count, .. } => {
                for i in 0..*count {
                    out.extend(COLUMN_FIELDS.iter().map(|(n, _, _)| cell(n, i)));
                }
            }
            Item::Text { .. } => {}
        }
    }
    out
}

/// Values with each [`Item::Lines`] item's lines joined under its own
/// name, and every entry's default where nothing is typed.
#[must_use]
pub fn gather(p: &Prompt, typed: &Values) -> Values {
    let mut v = typed.clone();
    for item in p.items {
        match item {
            Item::Lines { count, name, .. } => {
                let joined: Vec<String> = (0..*count)
                    .filter_map(|i| typed.get(&line(name, i)))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                v.insert((*name).to_string(), joined.join(" "));
            }
            Item::Entry { name, default, .. }
                if v.get(*name).is_none_or(|s| s.trim().is_empty()) && !default.is_empty() =>
            {
                v.insert((*name).to_string(), (*default).to_string());
            }
            _ => {}
        }
    }
    v
}

/// The prompt with `values` (a field not in it shows its default).
#[must_use]
pub fn render(p: &Prompt, values: &Values, sql_naming: bool, msg: Option<&str>) -> Screen {
    let pick = |l: &Label| if sql_naming { l.sql } else { l.sys };
    let get = |n: &str, d: &str| values.get(n).cloned().unwrap_or_else(|| d.to_string());
    let mut s = blank();
    title(&mut s, p.title_col, p.title);
    let mut first = None;
    for item in p.items {
        match item {
            Item::Entry {
                row,
                col,
                label,
                end,
                field,
                width,
                name,
                default,
                hints,
            } => {
                let text = pick(label);
                let shown = if *end == 0 {
                    text.to_string()
                } else {
                    leader(text, *col, *end)
                };
                put(&mut s, *row, *col, Attr::Normal, &shown);
                var(&mut s, *row, *field, *width, name, &get(name, default));
                for (i, (c, h)) in hints.iter().enumerate() {
                    put(&mut s, row + i, *c, Attr::Normal, h);
                }
                first.get_or_insert((*row, *field));
            }
            Item::Text {
                row,
                col,
                attr,
                text,
            } => put(&mut s, *row, *col, *attr, pick(text)),
            Item::Lines {
                row,
                col,
                width,
                count,
                name,
            } => {
                for i in 0..*count {
                    let n = line(name, i);
                    var(&mut s, row + i, *col, *width, &n, &get(&n, ""));
                }
                first.get_or_insert((*row, *col));
            }
            Item::Columns { row, count } => {
                let heads: [(usize, Label); 6] = [
                    (2, named("Field", "Column")),
                    (24, named("FOR Field", "FOR Column")),
                    (38, same("Type")),
                    (56, same("Length")),
                    (68, same("Scale")),
                    (75, same("Nulls")),
                ];
                for (c, h) in heads {
                    put(&mut s, row - 1, c, Attr::HighIntensity, pick(&h));
                }
                for i in 0..*count {
                    for (n, c, w) in COLUMN_FIELDS {
                        let name = cell(n, i);
                        let d = if *n == "NULLS" { "3" } else { "" };
                        var(&mut s, row + i, *c, *w, &name, &get(&name, d));
                    }
                }
            }
        }
    }
    if let Some(r) = p.bottom {
        more(&mut s, r, 79, true);
    }
    keys(&mut s, p.keys);
    message(&mut s, msg);
    s.cursor = first;
    s
}
