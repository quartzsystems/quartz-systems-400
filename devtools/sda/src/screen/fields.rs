// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work with Fields (F4 on the image), the Select File, Record and
//! Field Keywords displays, and the displays a keyword group opens:
//! display attributes and colors as Y/N lists, every other group as
//! lines of keyword text.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{Values, blank, input, keys, leader, message, more, put, title, var};
use crate::keywords::Group;
use crate::source::{Element, Kind, Usage};

/// The list's name in the screen's targets.
pub const LIST: &str = "FIELDS";
/// Entries on a page.
pub const PAGE: usize = 6;

/// One element on Work with Fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Order number.
    pub order: u32,
    /// The field's name, or a constant's text.
    pub name: String,
    /// `C` for a constant, else the data type.
    pub ty: char,
    /// The use, blank for a constant.
    pub usage: char,
    /// `8` or `7,2`.
    pub length: String,
    /// `03 002`, blank for a hidden field.
    pub at: String,
    /// `R` for a referenced field.
    pub reference: bool,
    /// The indicators conditioning it.
    pub condition: String,
    /// It overlaps another.
    pub overlap: bool,
}

impl Row {
    /// The row for an element.
    #[must_use]
    pub fn of(order: u32, e: &Element, overlap: bool) -> Self {
        let at = e
            .pos
            .filter(|_| e.displayed())
            .map(|(r, c)| format!("{r:02} {c:03}"))
            .unwrap_or_default();
        let condition = e
            .lines
            .iter()
            .find(|l| !l.is_comment())
            .map(|l| l.word(8, 16))
            .unwrap_or_default();
        match &e.kind {
            Kind::Constant { text, keyword } => Self {
                order,
                name: keyword.clone().unwrap_or_else(|| text.clone()),
                ty: 'C',
                usage: ' ',
                length: e.width.to_string(),
                at,
                reference: false,
                condition,
                overlap,
            },
            Kind::Field {
                name,
                usage,
                ty,
                len,
                dec,
                reference,
            } => Self {
                order,
                name: name.clone(),
                ty: match (ty, dec) {
                    (' ', None) => 'A',
                    (' ', Some(_)) => 'Y',
                    (t, _) => *t,
                },
                usage: usage.letter(),
                length: match dec {
                    Some(d) if *d > 0 => format!("{len},{d}"),
                    _ => len.to_string(),
                },
                at,
                reference: *reference,
                condition,
                overlap,
            },
        }
    }
}

/// Work with Fields: `ROLL`, the entries' options (`FIELDS` list),
/// order (`ORDER`), name (`NAME`), length (`LENGTH`) and position
/// (`AT`), and three add lines (`ADDH`, `ADDM`, `ADDP` with
/// `ADDxLEN`).
#[must_use]
pub fn work_with_fields(record: &str, rows: &[Row], top: usize, roll: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 33, "Work with Fields");
    put(&mut s, 3, 2, Attr::Normal, "Record . . . :");
    put(&mut s, 3, 19, Attr::HighIntensity, record);
    put(&mut s, 5, 2, Attr::Blue, "Type information, press Enter.");
    put(&mut s, 6, 4, Attr::Normal, &leader("Number of fields to roll", 4, 63));
    var(&mut s, 6, 67, 3, "ROLL", &format!("{roll:>3}"));
    put(&mut s, 8, 2, Attr::Blue, "Type options, change values, press Enter.");
    put(&mut s, 9, 4, Attr::Blue, "1=Select keywords");
    put(&mut s, 9, 23, Attr::Blue, "2=Edit comments");
    put(&mut s, 9, 41, Attr::Blue, "4=Delete field");
    for (col, t) in [
        (2, "Option"),
        (11, "Order"),
        (19, "Field"),
        (31, "Type Use"),
        (41, "Length"),
        (49, "Row/Col"),
        (58, "Ref Condition"),
        (73, "Overlap"),
    ] {
        put(&mut s, 11, col, Attr::HighIntensity, t);
    }
    for (i, r) in rows.iter().enumerate().skip(top).take(PAGE) {
        let row = 12 + i - top;
        input(
            &mut s,
            row,
            4,
            2,
            Target::ListOption {
                list: LIST.into(),
                row_index: i,
            },
            "",
        );
        input(
            &mut s,
            row,
            11,
            5,
            Target::ListCell {
                list: LIST.into(),
                row_index: i,
                var: "ORDER".into(),
            },
            &format!("{:>5}", r.order),
        );
        let name: String = r.name.chars().take(12).collect();
        if r.ty == 'C' {
            put(&mut s, row, 19, Attr::Normal, &name);
        } else {
            input(
                &mut s,
                row,
                19,
                10,
                Target::ListCell {
                    list: LIST.into(),
                    row_index: i,
                    var: "NAME".into(),
                },
                &name,
            );
            put(&mut s, row, 37, Attr::Normal, &r.usage.to_string());
        }
        put(&mut s, row, 33, Attr::Normal, &r.ty.to_string());
        let lc = 47 - r.length.chars().count();
        put(&mut s, row, lc, Attr::Normal, &r.length);
        put(&mut s, row, 50, Attr::Normal, &r.at);
        if r.reference {
            put(&mut s, row, 58, Attr::Normal, "R");
        }
        put(&mut s, row, 62, Attr::Normal, &r.condition);
        if r.overlap {
            put(&mut s, row, 75, Attr::HighIntensity, "Y");
        }
    }
    more(&mut s, 18, top + PAGE >= rows.len());
    for (row, usage, text, name) in [
        (19, "H", "Hidden", "ADDH"),
        (20, "M", "Message", "ADDM"),
        (21, "P", "Program-to-system", "ADDP"),
    ] {
        put(&mut s, row, 3, Attr::Normal, "Add");
        var(&mut s, row, 19, 10, name, "");
        put(&mut s, row, 37, Attr::Normal, usage);
        var(&mut s, row, 41, 7, &format!("{name}LEN"), "");
        put(&mut s, row, 50, Attr::Normal, text);
    }
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F6=Sort by row/column"), (23, 36, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((12, 4));
    s
}

/// What heads a Select Keywords display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// The member (file level).
    Member(String),
    /// The record.
    Record(String),
    /// A field or constant.
    Element {
        /// `Field` or `Constant`.
        constant: bool,
        /// Its name or text.
        name: String,
        /// Its use, for a field.
        usage: Option<char>,
        /// `8` or `7,2`.
        length: String,
        /// Row and column.
        at: Option<(u16, u16)>,
    },
}

impl Head {
    /// The head of an element's display.
    #[must_use]
    pub fn of(e: &Element) -> Self {
        let row = Row::of(0, e, false);
        match &e.kind {
            Kind::Constant { .. } => Self::Element {
                constant: true,
                name: row.name,
                usage: None,
                length: row.length,
                at: e.pos,
            },
            Kind::Field { usage, .. } => Self::Element {
                constant: false,
                name: row.name,
                usage: Some(usage.letter()),
                length: row.length,
                at: e.pos.filter(|_| *usage != Usage::Hidden),
            },
        }
    }
}

/// Select File, Record or Field Keywords: a `Y` field per group,
/// named by the group's id.
#[must_use]
pub fn select_keywords(head: &Head, groups: &[Group], v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    let field_level = matches!(head, Head::Element { .. });
    match head {
        Head::Member(m) => {
            title(&mut s, 30, "Select File Keywords");
            put(&mut s, 3, 2, Attr::Normal, "Member . . . :");
            put(&mut s, 3, 19, Attr::HighIntensity, m);
        }
        Head::Record(r) => {
            title(&mut s, 29, "Select Record Keywords");
            put(&mut s, 3, 2, Attr::Normal, "Record . . . :");
            put(&mut s, 3, 19, Attr::HighIntensity, r);
        }
        Head::Element {
            constant,
            name,
            usage,
            length,
            at,
        } => {
            title(&mut s, 30, "Select Field Keywords");
            if *constant {
                put(&mut s, 3, 2, Attr::Normal, "Constant  . . . :");
            } else {
                put(&mut s, 3, 2, Attr::Normal, "Field . . . . . :");
            }
            put(&mut s, 3, 22, Attr::HighIntensity, name);
            if let Some(u) = usage {
                put(&mut s, 3, 43, Attr::Normal, "Usage . . :");
                put(&mut s, 3, 56, Attr::HighIntensity, &u.to_string());
            }
            put(&mut s, 4, 2, Attr::Normal, "Length  . . . . :");
            put(&mut s, 4, 22, Attr::HighIntensity, length);
            if let Some((r, c)) = at {
                put(&mut s, 4, 43, Attr::Normal, "Row . . . :");
                put(&mut s, 4, 56, Attr::HighIntensity, &r.to_string());
                put(&mut s, 4, 61, Attr::Normal, "Column . . . :");
                put(&mut s, 4, 77, Attr::HighIntensity, &c.to_string());
            }
        }
    }
    let first = if field_level { 6 } else { 5 };
    put(&mut s, first, 2, Attr::Blue, "Type choices, press Enter.");
    put(&mut s, 7, 38, Attr::HighIntensity, "Y=Yes");
    if field_level {
        put(&mut s, 7, 46, Attr::HighIntensity, "For Field Type");
    }
    for g in groups {
        put(&mut s, g.row, 4, Attr::Normal, &leader(g.label, 4, 36));
        var(&mut s, g.row, 40, 1, g.id, &v(g.id));
        put(&mut s, g.row, 46, Attr::Normal, g.for_type);
    }
    keys(
        &mut s,
        &[
            (23, 2, "F3=Exit"),
            (23, 12, "F4=Display Selected Keywords"),
            (23, 43, "F12=Cancel"),
        ],
    );
    message(&mut s, msg);
    s.cursor = groups.first().map(|g| (g.row, 40));
    s
}

/// The display attributes `DSPATR` takes without a field name, in the
/// order the Y/N list shows them.
pub const ATTRS: &[(&str, &str)] = &[
    ("HI", "High intensity"),
    ("RI", "Reverse image"),
    ("CS", "Column separators"),
    ("BL", "Blink"),
    ("ND", "Nondisplay"),
    ("UL", "Underline"),
    ("PC", "Position cursor"),
    ("PR", "Protect field"),
    ("MDT", "Set modified data tag"),
];

/// The colors `COLOR` takes.
pub const COLORS: &[(&str, &str)] = &[
    ("BLU", "Blue"),
    ("GRN", "Green"),
    ("PNK", "Pink"),
    ("RED", "Red"),
    ("TRQ", "Turquoise"),
    ("WHT", "White"),
    ("YLW", "Yellow"),
];

/// A Y/N list for display attributes or colors: one field per value,
/// named by the value.
#[must_use]
pub fn choose(title_text: &str, what: &str, items: &[(&str, &str)], v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    let col = 1 + (80 - title_text.chars().count()) / 2;
    title(&mut s, col, title_text);
    put(&mut s, 3, 2, Attr::Normal, &leader("For", 2, 16));
    put(&mut s, 3, 19, Attr::HighIntensity, what);
    put(&mut s, 5, 2, Attr::Blue, "Type choices, press Enter.");
    put(&mut s, 7, 38, Attr::HighIntensity, "Y=Yes");
    for (i, (value, label)) in items.iter().enumerate() {
        let row = 8 + i;
        put(&mut s, row, 4, Attr::Normal, &leader(label, 4, 36));
        var(&mut s, row, 40, 1, value, &v(value));
        put(&mut s, row, 46, Attr::Normal, value);
    }
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((8, 40));
    s
}

/// Lines of a keyword group shown as text to change (`KW0`..`KW11`).
pub const KEYWORD_LINES: usize = 12;

/// The keywords of one group as editable lines.
#[must_use]
pub fn keyword_text(title_text: &str, what: &str, lines: &[String], msg: Option<&str>) -> Screen {
    let mut s = blank();
    let col = 1 + (80 - title_text.chars().count()) / 2;
    title(&mut s, col, title_text);
    put(&mut s, 3, 2, Attr::Normal, &leader("For", 2, 16));
    put(&mut s, 3, 19, Attr::HighIntensity, what);
    put(&mut s, 5, 2, Attr::Blue, "Type keywords and their parameters, press Enter.");
    for i in 0..KEYWORD_LINES {
        let v = lines.get(i).map_or("", String::as_str);
        var(&mut s, 7 + i, 4, 74, &format!("KW{i}"), v);
    }
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((7, 4));
    s
}
