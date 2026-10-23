// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The three lists and Work with User-Defined Options: the header with
//! its fields, the option legends of the set F23 shows, the entries a
//! page at a time in the view F11/F14 picks, the command line and the
//! key set F24 shows.

use std::collections::BTreeMap;

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, bottom_line, input, keys, leader, more, put, title, var};
use crate::option::{ListKind, options};

/// The list name the option fields carry.
pub const LIST: &str = "PDM";

/// How the entries show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// Name, type, attribute or date, text.
    #[default]
    Full,
    /// Names and types in columns (F11).
    Names,
    /// Members: the change date instead of the type (F14).
    Date,
    /// Objects: the size instead of the attribute (F14).
    Size,
}

/// An entry as the list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// The name.
    pub name: String,
    /// The type (`*PROD`, `*PGM`, `CLLE`).
    pub ty: String,
    /// The third column: ASP device, attribute.
    pub third: String,
    /// The change date, `MM/DD/YY`.
    pub date: String,
    /// The size.
    pub size: u64,
    /// The text.
    pub text: String,
}

/// Legend positions of an option set: two rows of `(column, option)`.
type Legends = [&'static [(usize, u8)]; 2];

const LIB_OPTS: &[Legends] = &[
    [
        &[(4, 2), (20, 3), (49, 4), (65, 5)],
        &[(4, 7), (20, 8), (49, 9), (65, 10)],
    ],
    [&[(4, 12), (32, 13)], &[]],
];

const OBJ_OPTS: &[Legends] = &[
    [
        &[(4, 2), (19, 3), (33, 4), (47, 5), (63, 7)],
        &[(4, 8), (33, 9), (46, 10), (62, 11)],
    ],
    [
        &[(4, 12), (29, 13), (54, 15)],
        &[(4, 16), (29, 18), (54, 25)],
    ],
    [&[(4, 26), (38, 27)], &[(4, 34), (38, 54)]],
];

const MBR_OPTS: &[Legends] = &[
    [
        &[(3, 2), (18, 3), (26, 4), (35, 5), (51, 6), (63, 7)],
        &[(3, 8), (26, 9), (34, 13), (50, 14), (62, 15)],
    ],
    [
        &[(4, 16), (24, 17), (50, 19)],
        &[(4, 25), (24, 54), (50, 55)],
    ],
];

/// Key positions of a key set: two rows of `(column, text)`.
type KeySet = [&'static [(usize, &'static str)]; 2];

const LIB_KEYS: &[KeySet] = &[
    [
        &[
            (2, "F3=Exit"),
            (19, "F4=Prompt"),
            (40, "F5=Refresh"),
            (61, "F6=Create"),
        ],
        &[
            (2, "F9=Retrieve"),
            (19, "F10=Command entry"),
            (40, "F23=More options"),
            (61, "F24=More keys"),
        ],
    ],
    [
        &[
            (2, "F11=Display names only"),
            (31, "F12=Cancel"),
            (54, "F13=Repeat"),
        ],
        &[
            (2, "F16=User options"),
            (31, "F23=More options"),
            (54, "F24=More keys"),
        ],
    ],
    [
        &[(2, "F17=Subset"), (28, "F18=Change defaults")],
        &[
            (2, "F21=Print list"),
            (28, "F23=More options"),
            (53, "F24=More keys"),
        ],
    ],
];

const OBJ_KEYS: &[KeySet] = &[
    LIB_KEYS[0],
    [
        &[
            (2, "F11=Display names and types"),
            (33, "F12=Cancel"),
            (58, "F13=Repeat"),
        ],
        &[
            (2, "F14=Display Size"),
            (33, "F23=More options"),
            (58, "F24=More keys"),
        ],
    ],
    [
        &[
            (2, "F16=User options"),
            (28, "F17=Subset"),
            (53, "F18=Change defaults"),
        ],
        &[
            (2, "F21=Print list"),
            (28, "F23=More options"),
            (53, "F24=More keys"),
        ],
    ],
];

const MBR_KEYS: &[KeySet] = &[
    [
        &[
            (2, "F3=Exit"),
            (19, "F4=Prompt"),
            (41, "F5=Refresh"),
            (63, "F6=Create"),
        ],
        &[
            (2, "F9=Retrieve"),
            (19, "F10=Command entry"),
            (41, "F23=More options"),
            (63, "F24=More keys"),
        ],
    ],
    [
        &[
            (2, "F11=Display names and types"),
            (45, "F12=Cancel"),
            (64, "F13=Repeat"),
        ],
        &[
            (2, "F14=Display date"),
            (26, "F15=Sort date"),
            (45, "F23=More options"),
            (64, "F24=More keys"),
        ],
    ],
    [
        &[
            (2, "F16=User options"),
            (22, "F17=Subset"),
            (37, "F18=Change defaults"),
            (61, "F21=Print list"),
        ],
        &[(37, "F23=More options"), (61, "F24=More keys")],
    ],
];

/// The option sets of a list.
fn option_sets(kind: ListKind) -> &'static [Legends] {
    match kind {
        ListKind::Library => LIB_OPTS,
        ListKind::Object => OBJ_OPTS,
        ListKind::Member => MBR_OPTS,
    }
}

/// The key sets of a list.
fn key_sets(kind: ListKind) -> &'static [KeySet] {
    match kind {
        ListKind::Library => LIB_KEYS,
        ListKind::Object => OBJ_KEYS,
        ListKind::Member => MBR_KEYS,
    }
}

/// How many option sets F23 goes through.
#[must_use]
pub fn option_set_count(kind: ListKind) -> usize {
    option_sets(kind).len()
}

/// How many key sets F24 goes through.
#[must_use]
pub fn key_set_count(kind: ListKind) -> usize {
    key_sets(kind).len()
}

/// The first row of entries, the rows a page has and the columns of
/// the names view.
fn geometry(kind: ListKind) -> (usize, usize, usize) {
    match kind {
        ListKind::Library => (10, 9, 4),
        ListKind::Object | ListKind::Member => (11, 8, 3),
    }
}

/// Entries a page shows.
#[must_use]
pub fn page_size(kind: ListKind, view: View) -> usize {
    let (_, rows, cols) = geometry(kind);
    if view == View::Names {
        rows * cols
    } else {
        rows
    }
}

/// A list display's contents.
pub struct ListScreen<'a> {
    /// Which list.
    pub kind: ListKind,
    /// The system name.
    pub system: &'a str,
    /// Header values: `LISTTYPE`; `LIB`; `FILE`, `LIB`; and `POSTO`,
    /// `POSTYPE`.
    pub header: &'a BTreeMap<String, String>,
    /// The option set (F23).
    pub option_set: usize,
    /// The key set (F24).
    pub key_set: usize,
    /// The view.
    pub view: View,
    /// Every entry.
    pub rows: &'a [Row],
    /// The first entry on the page.
    pub top: usize,
    /// Options typed, by entry.
    pub options: &'a BTreeMap<usize, String>,
    /// The command line.
    pub cmdline: &'a str,
    /// The message line.
    pub message: Option<&'a str>,
    /// The copyright line (the session's first display).
    pub copyright: bool,
}

/// Put an option set's legends.
fn legends(s: &mut Screen, kind: ListKind, set: usize, first_row: usize) {
    let sets = option_sets(kind);
    let Some(rows) = sets.get(set % sets.len()) else {
        return;
    };
    let all = options(kind);
    let last_row = usize::from(!rows[1].is_empty());
    for (r, items) in rows.iter().enumerate() {
        for (i, (col, n)) in items.iter().enumerate() {
            let Some(o) = all.iter().find(|o| o.number == *n) else {
                continue;
            };
            let mut text = o.legend.to_string();
            if sets.len() > 1 && r == last_row && i + 1 == items.len() {
                let end = col + text.len() + 3;
                text.push_str(if end <= 79 { " ..." } else { "..." });
            }
            put(s, first_row + r, *col, Attr::Blue, &text);
        }
    }
}

/// The header rows, by list.
fn header(s: &mut Screen, l: &ListScreen<'_>) {
    let h = |k: &str| l.header.get(k).cloned().unwrap_or_default();
    match l.kind {
        ListKind::Library => {
            title(s, 27, "Work with Libraries Using PDM", Some(l.system));
            put(s, 3, 2, Attr::Normal, &leader("List type", 2, 25));
            var(s, 3, 29, 10, "LISTTYPE", &h("LISTTYPE"));
            put(s, 3, 45, Attr::Normal, &leader("Position to", 45, 65));
            input(
                s,
                3,
                69,
                10,
                Target::PositionTo("POSTO".into()),
                &h("POSTO"),
            );
        }
        ListKind::Object => {
            title(s, 27, "Work with Objects Using PDM", Some(l.system));
            put(s, 3, 2, Attr::Normal, &leader("Library", 2, 18));
            var(s, 3, 22, 10, "LIB", &h("LIB"));
            put(s, 3, 39, Attr::Normal, &leader("Position to", 39, 65));
            input(
                s,
                3,
                69,
                10,
                Target::PositionTo("POSTO".into()),
                &h("POSTO"),
            );
            put(s, 4, 39, Attr::Normal, &leader("Position to type", 39, 65));
            input(
                s,
                4,
                69,
                10,
                Target::PositionTo("POSTYPE".into()),
                &h("POSTYPE"),
            );
        }
        ListKind::Member => {
            title(s, 28, "Work with Members Using PDM", Some(l.system));
            put(s, 3, 2, Attr::Normal, &leader("File", 2, 18));
            var(s, 3, 22, 10, "FILE", &h("FILE"));
            put(s, 4, 4, Attr::Normal, &leader("Library", 4, 18));
            var(s, 4, 24, 10, "LIB", &h("LIB"));
            put(s, 4, 45, Attr::Normal, &leader("Position to", 45, 66));
            input(
                s,
                4,
                70,
                10,
                Target::PositionTo("POSTO".into()),
                &h("POSTO"),
            );
        }
    }
}

/// The column headings of the full views.
fn headings(s: &mut Screen, l: &ListScreen<'_>, row: usize) {
    let hi = Attr::HighIntensity;
    put(s, row, 2, hi, "Opt");
    match l.kind {
        ListKind::Library => {
            put(s, row - 1, 31, hi, "ASP");
            put(s, row, 7, hi, "Library");
            put(s, row, 19, hi, "Type");
            put(s, row, 31, hi, "Device");
            put(s, row, 43, hi, "Text");
        }
        ListKind::Object => {
            put(s, row, 7, hi, "Object");
            put(s, row, 19, hi, "Type");
            put(
                s,
                row,
                31,
                hi,
                if l.view == View::Size {
                    "Size"
                } else {
                    "Attribute"
                },
            );
            put(s, row, 43, hi, "Text");
        }
        ListKind::Member => {
            put(s, row, 7, hi, "Member");
            put(
                s,
                row,
                19,
                hi,
                if l.view == View::Date { "Date" } else { "Type" },
            );
            put(s, row, 31, hi, "Text");
        }
    }
}

/// One entry of a full view.
fn full_row(s: &mut Screen, l: &ListScreen<'_>, row: usize, r: &Row) {
    let n = Attr::Normal;
    s.put(row, 7, n, &r.name);
    match l.kind {
        ListKind::Library => {
            s.put(row, 19, n, &r.ty);
            s.put(row, 31, n, &r.third);
            s.put(row, 43, n, &r.text);
        }
        ListKind::Object => {
            s.put(row, 19, n, &r.ty);
            if l.view == View::Size {
                s.put(row, 31, n, &format!("{:>10}", r.size));
            } else {
                s.put(row, 31, n, &r.third);
            }
            s.put(row, 43, n, &r.text);
        }
        ListKind::Member => {
            s.put(
                row,
                19,
                n,
                if l.view == View::Date { &r.date } else { &r.ty },
            );
            s.put(row, 31, n, &r.text);
        }
    }
}

/// The text shown when the list has no entries.
fn empty_text(kind: ListKind) -> &'static str {
    match kind {
        ListKind::Library => "(No libraries in list)",
        ListKind::Object => "(No objects in library)",
        ListKind::Member => "(No members in file)",
    }
}

/// The list display.
#[must_use]
pub fn render(l: &ListScreen<'_>) -> Screen {
    let mut s = blank();
    header(&mut s, l);
    let (first, per_col, cols) = geometry(l.kind);
    let instr_row = if l.kind == ListKind::Library { 5 } else { 6 };
    put(
        &mut s,
        instr_row,
        2,
        Attr::Blue,
        "Type options, press Enter.",
    );
    legends(&mut s, l.kind, l.option_set, instr_row + 1);
    let head_row = first - 1;
    let page = page_size(l.kind, l.view);
    let shown: Vec<(usize, &Row)> = l.rows.iter().enumerate().skip(l.top).take(page).collect();
    if l.view == View::Names {
        let width = if l.kind == ListKind::Library { 19 } else { 26 };
        let used = shown.len().div_ceil(per_col).max(1);
        let label = match l.kind {
            ListKind::Library => "Library",
            ListKind::Object => "Object",
            ListKind::Member => "Member",
        };
        for c in 0..used.min(cols) {
            let col = 2 + c * width;
            if l.kind == ListKind::Library {
                put(&mut s, head_row, col, Attr::HighIntensity, "Opt");
                put(&mut s, head_row, col + 5, Attr::HighIntensity, label);
            } else {
                put(
                    &mut s,
                    head_row,
                    col,
                    Attr::HighIntensity,
                    &format!("Opt {label}"),
                );
                put(&mut s, head_row, col + 15, Attr::HighIntensity, "Type");
            }
        }
        for (k, (i, r)) in shown.iter().enumerate() {
            let col = 2 + (k / per_col) * width;
            let row = first + k % per_col;
            let opt = l.options.get(i).cloned().unwrap_or_default();
            input(&mut s, row, col, 2, opt_target(*i), &opt);
            if l.kind == ListKind::Library {
                put(&mut s, row, col + 5, Attr::Normal, &r.name);
            } else {
                put(&mut s, row, col + 4, Attr::Normal, &r.name);
                put(&mut s, row, col + 15, Attr::Normal, &r.ty);
            }
        }
    } else {
        headings(&mut s, l, head_row);
        for (k, (i, r)) in shown.iter().enumerate() {
            let row = first + k;
            let opt = l.options.get(i).cloned().unwrap_or_default();
            input(&mut s, row, 2, 2, opt_target(*i), &opt);
            full_row(&mut s, l, row, r);
        }
    }
    if l.rows.is_empty() {
        put(&mut s, first + 1, 4, Attr::Normal, empty_text(l.kind));
    } else {
        more(&mut s, 19, l.top + page >= l.rows.len());
    }
    put(&mut s, 20, 2, Attr::Normal, "Parameters or command");
    put(&mut s, 21, 2, Attr::Normal, "===>");
    input(&mut s, 21, 7, 73, Target::CmdLine, l.cmdline);
    let ks = key_sets(l.kind);
    let set = &ks[l.key_set % ks.len()];
    for (r, items) in set.iter().enumerate() {
        for (c, t) in *items {
            put(&mut s, 22 + r, *c, Attr::Blue, t);
        }
    }
    bottom_line(&mut s, l.message, l.copyright);
    s.cursor = Some((first, 2));
    s
}

/// The target of an entry's option field.
fn opt_target(i: usize) -> Target {
    Target::ListOption {
        list: LIST.into(),
        row_index: i,
    }
}

/// Work with User-Defined Options' contents.
pub struct OptionsScreen<'a> {
    /// The system name.
    pub system: &'a str,
    /// File, library, member.
    pub file: (&'a str, &'a str, &'a str),
    /// The position-to value.
    pub position: &'a str,
    /// Code and command, every option.
    pub rows: &'a [(String, String)],
    /// The first on the page.
    pub top: usize,
    /// Options typed.
    pub options: &'a BTreeMap<usize, String>,
    /// The key set (F24).
    pub key_set: usize,
    /// The command line.
    pub cmdline: &'a str,
    /// The message line.
    pub message: Option<&'a str>,
}

/// Entries a page of the user-defined options shows.
pub const OPTIONS_PAGE: usize = 9;

/// Work with User-Defined Options.
#[must_use]
pub fn user_options(o: &OptionsScreen<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 24, "Work with User-Defined Options", Some(o.system));
    put(&mut s, 3, 2, Attr::Normal, &leader("File", 2, 19));
    put(&mut s, 3, 21, Attr::Normal, ":");
    put(&mut s, 3, 25, Attr::Normal, o.file.0);
    put(&mut s, 3, 42, Attr::Normal, &leader("Member", 42, 59));
    put(&mut s, 3, 61, Attr::Normal, ":");
    put(&mut s, 3, 65, Attr::Normal, o.file.2);
    put(&mut s, 4, 4, Attr::Normal, &leader("Library", 4, 19));
    put(&mut s, 4, 21, Attr::Normal, ":");
    put(&mut s, 4, 27, Attr::Normal, o.file.1);
    put(&mut s, 4, 42, Attr::Normal, &leader("Position to", 42, 59));
    put(&mut s, 4, 61, Attr::Normal, ":");
    input(
        &mut s,
        4,
        65,
        2,
        Target::PositionTo("POSTO".into()),
        o.position,
    );
    put(&mut s, 6, 2, Attr::Blue, "Type options, press Enter.");
    for (c, t) in [
        (4, "2=Change"),
        (21, "3=Copy"),
        (36, "4=Delete"),
        (53, "5=Display"),
    ] {
        put(&mut s, 7, c, Attr::Blue, t);
    }
    put(&mut s, 9, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 9, 7, Attr::HighIntensity, "Option");
    put(&mut s, 9, 15, Attr::HighIntensity, "Command");
    for (k, (i, (code, cmd))) in o
        .rows
        .iter()
        .enumerate()
        .skip(o.top)
        .take(OPTIONS_PAGE)
        .enumerate()
    {
        let row = 10 + k;
        let opt = o.options.get(&i).cloned().unwrap_or_default();
        input(&mut s, row, 3, 1, opt_target(i), &opt);
        put(&mut s, row, 11 - code.len(), Attr::Normal, code);
        put(&mut s, row, 15, Attr::Normal, cmd);
    }
    if o.rows.is_empty() {
        put(&mut s, 11, 4, Attr::Normal, "(No options in file)");
    } else {
        more(&mut s, 19, o.top + OPTIONS_PAGE >= o.rows.len());
    }
    put(&mut s, 20, 2, Attr::Normal, "Command");
    put(&mut s, 21, 2, Attr::Normal, "===>");
    input(&mut s, 21, 7, 73, Target::CmdLine, o.cmdline);
    let sets: [&[(usize, usize, &str)]; 2] = [
        &[
            (22, 2, "F3=Exit"),
            (22, 22, "F4=Prompt"),
            (22, 42, "F5=Refresh"),
            (22, 59, "F6=Create"),
            (23, 2, "F9=Retrieve"),
            (23, 22, "F10=Command entry"),
            (23, 59, "F24=More keys"),
        ],
        &[
            (22, 2, "F12=Cancel"),
            (22, 26, "F13=Repeat"),
            (22, 46, "F15=Exit without saving changes"),
            (23, 2, "F18=Change defaults"),
            (23, 46, "F24=More keys"),
        ],
    ];
    keys(&mut s, sets[o.key_set % 2]);
    bottom_line(&mut s, o.message, false);
    s.cursor = Some((10, 3));
    s
}
