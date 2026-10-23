// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry displays: the Specify panels behind the menu's options,
//! Change Defaults, the two subset panels, Find String, and the small
//! displays an option asks for (confirm, rename, new text, a
//! user-defined option).

use uim::Screen;
use uim::render::Attr;

use super::{blank, bottom_line, keys, leader, more, put, title, var};

/// The values of a display's fields, by name.
pub type Values<'a> = &'a dyn Fn(&str) -> String;

/// One entry row: a label with leaders to `end`, a field, a hint.
struct Entry<'a> {
    row: usize,
    col: usize,
    label: &'a str,
    end: usize,
    field: usize,
    len: usize,
    name: &'a str,
    hint: (usize, &'a str),
}

fn entry(s: &mut Screen, e: &Entry<'_>, v: Values<'_>) {
    put(
        s,
        e.row,
        e.col,
        Attr::Normal,
        &leader(e.label, e.col, e.end),
    );
    var(s, e.row, e.field, e.len, e.name, &v(e.name));
    put(s, e.row, e.hint.0, Attr::Normal, e.hint.1);
}

/// The common head: the title and the instruction.
fn head(s: &mut Screen, col: usize, text: &str, choices: bool) {
    title(s, col, text, None);
    put(
        s,
        3,
        2,
        Attr::Blue,
        if choices {
            "Type choices, press Enter."
        } else {
            "Type choice, press Enter."
        },
    );
}

/// The keys most entry displays show on row 23.
const EXIT_REFRESH_CANCEL: &[(usize, usize, &str)] = &[
    (23, 2, "F3=Exit"),
    (23, 14, "F5=Refresh"),
    (23, 29, "F12=Cancel"),
];

/// The same with F4.
const EXIT_PROMPT_REFRESH_CANCEL: &[(usize, usize, &str)] = &[
    (23, 2, "F3=Exit"),
    (23, 14, "F4=Prompt"),
    (23, 28, "F5=Refresh"),
    (23, 43, "F12=Cancel"),
];

/// Specify Libraries to Work With: `LIB`, `ASPNUM`, `ASPDEV`.
#[must_use]
pub fn specify_libs(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 25, "Specify Libraries to Work With", false);
    let rows = [
        (5, "Library", "LIB", "*LIBL, name, *generic*, *ALL,"),
        (7, "ASP number", "ASPNUM", "1-32, *ALL"),
        (8, "ASP device", "ASPDEV", "Name, *, *SYSBAS, *CURASPGRP"),
    ];
    for (row, label, name, hint) in rows {
        entry(
            &mut s,
            &Entry {
                row,
                col: 4,
                label,
                end: 33,
                field: 37,
                len: 10,
                name,
                hint: (50, hint),
            },
            v,
        );
    }
    put(&mut s, 6, 50, Attr::Normal, "*ALLUSR, *USRLIBL, *CURLIB");
    keys(&mut s, EXIT_REFRESH_CANCEL);
    bottom_line(&mut s, message, false);
    s
}

/// Specify Objects to Work With: `LIB`, `OBJ`, `OBJTYPE`, `OBJATR`.
#[must_use]
pub fn specify_objs(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 24, "Specify Objects to Work With", true);
    let e = |row, col, label, name, hint| Entry {
        row,
        col,
        label,
        end: 31,
        field: 35,
        len: 10,
        name,
        hint: (48, hint),
    };
    entry(&mut s, &e(5, 4, "Library", "LIB", "*CURLIB, name"), v);
    put(&mut s, 7, 4, Attr::Normal, "Object:");
    entry(&mut s, &e(8, 6, "Name", "OBJ", "*ALL, name, *generic*"), v);
    entry(&mut s, &e(9, 6, "Type", "OBJTYPE", "*ALL, *type"), v);
    entry(
        &mut s,
        &e(10, 6, "Attribute", "OBJATR", "*ALL, attribute, *generic*,"),
        v,
    );
    put(&mut s, 11, 48, Attr::Normal, "*BLANK");
    keys(&mut s, EXIT_REFRESH_CANCEL);
    bottom_line(&mut s, message, false);
    s
}

/// Specify Members to Work With: `FILE`, `LIB`, `MBR`, `MBRTYPE`.
#[must_use]
pub fn specify_mbrs(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 24, "Specify Members to Work With", true);
    let e = |row, col, label, field, name, hint| Entry {
        row,
        col,
        label,
        end: 28,
        field,
        len: 10,
        name,
        hint: (45, hint),
    };
    entry(&mut s, &e(5, 4, "File", 32, "FILE", "Name, F4 for list"), v);
    entry(
        &mut s,
        &e(7, 6, "Library", 34, "LIB", "*LIBL, *CURLIB, name"),
        v,
    );
    put(&mut s, 9, 4, Attr::Normal, "Member:");
    entry(
        &mut s,
        &e(10, 6, "Name", 32, "MBR", "*ALL, name, *generic*"),
        v,
    );
    entry(
        &mut s,
        &e(
            11,
            6,
            "Type",
            32,
            "MBRTYPE",
            "*ALL, type, *generic*, *BLANK",
        ),
        v,
    );
    keys(&mut s, EXIT_PROMPT_REFRESH_CANCEL);
    bottom_line(&mut s, message, false);
    s
}

/// Specify Option File to Work With: `OPTFILE`, `OPTLIB`, `OPTMBR`.
#[must_use]
pub fn specify_optfile(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 23, "Specify Option File to Work With", true);
    let e = |row, col, label, field, name, hint| Entry {
        row,
        col,
        label,
        end: 28,
        field,
        len: 10,
        name,
        hint: (45, hint),
    };
    entry(
        &mut s,
        &e(5, 4, "File", 32, "OPTFILE", "Name, F4 for list"),
        v,
    );
    entry(
        &mut s,
        &e(7, 6, "Library", 34, "OPTLIB", "*LIBL, *CURLIB, name"),
        v,
    );
    entry(&mut s, &e(9, 4, "Member", 32, "OPTMBR", "Name"), v);
    keys(&mut s, EXIT_REFRESH_CANCEL);
    bottom_line(&mut s, message, false);
    s
}

/// Change Defaults, page 1 or 2.
#[must_use]
pub fn defaults(v: Values<'_>, page: usize, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 33, "Change Defaults", true);
    let yn = "Y=Yes, N=No";
    if page == 0 {
        let rows: [(usize, usize, &str, &str, &str); 13] = [
            (5, 4, "Object library", "OBJLIB", "Name, *CURLIB, *SRCLIB"),
            (6, 4, "Replace object", "REPLACE", yn),
            (7, 4, "Compile in batch", "BCHCMP", yn),
            (8, 4, "Run in batch", "BCHRUN", yn),
            (9, 4, "Save session defaults", "SAVSES", yn),
            (10, 4, "Save/Restore option", "SAVRST", "1=Single, 2=All"),
            (
                11,
                4,
                "Job description",
                "JOBD",
                "Name, *USRPRF, F4 for list",
            ),
            (12, 6, "Library", "JOBDLIB", "Name, *CURLIB, *LIBL"),
            (13, 4, "Change type and text", "CHGTXT", yn),
            (14, 4, "Option file", "OPTFILE", "Name"),
            (15, 6, "Library", "OPTLIB", "Name, *CURLIB, *LIBL"),
            (16, 4, "Member", "OPTMBR", "Name"),
            (17, 4, "Full screen mode", "FULLSCR", yn),
        ];
        for (row, col, label, name, hint) in rows {
            let field = if col == 6 { 37 } else { 35 };
            entry(
                &mut s,
                &Entry {
                    row,
                    col,
                    label,
                    end: 31,
                    field,
                    len: 10,
                    name,
                    hint: (48, hint),
                },
                v,
            );
        }
        more(&mut s, 22, false);
        keys(&mut s, EXIT_PROMPT_REFRESH_CANCEL);
    } else {
        let rows = [
            (5, "Log option commands", "LOGCMD"),
            (6, "Exit lists on ENTER  ", "EXITENT"),
        ];
        for (row, label, name) in rows {
            entry(
                &mut s,
                &Entry {
                    row,
                    col: 4,
                    label,
                    end: 32,
                    field: 35,
                    len: 10,
                    name,
                    hint: (48, yn),
                },
                v,
            );
        }
        more(&mut s, 22, true);
        keys(
            &mut s,
            &[
                (23, 3, "F3=Exit"),
                (23, 14, "F5=Refresh"),
                (23, 29, "F12=Cancel"),
            ],
        );
    }
    bottom_line(&mut s, message, false);
    s
}

/// The subset panels' keys.
const SUBSET_KEYS: &[(usize, usize, &str)] = &[
    (23, 2, "F3=Exit"),
    (23, 17, "F5=Refresh"),
    (23, 35, "F12=Cancel"),
];

/// Subset Object List: `OBJ`, `OBJTYPE`, `OBJATR`, `SIZEFROM`,
/// `SIZETO`, `TEXT`.
#[must_use]
pub fn subset_objs(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 31, "Subset Object List", true);
    let e = |row, col, label, name, len, hint| Entry {
        row,
        col,
        label,
        end: 33,
        field: 37,
        len,
        name,
        hint: (50, hint),
    };
    entry(
        &mut s,
        &e(5, 4, "Object", "OBJ", 10, "*ALL, name, *generic*"),
        v,
    );
    entry(
        &mut s,
        &e(7, 4, "Object type", "OBJTYPE", 10, "*ALL, *type"),
        v,
    );
    entry(
        &mut s,
        &e(
            9,
            4,
            "Object attribute",
            "OBJATR",
            10,
            "*ALL, attribute, *generic*,",
        ),
        v,
    );
    put(&mut s, 10, 50, Attr::Normal, "*BLANK");
    put(&mut s, 11, 4, Attr::Normal, "Object size");
    for (row, label, name) in [(12, "From", "SIZEFROM"), (13, "To", "SIZETO")] {
        let value = v(name);
        let shown = format!("{:>10}", value.trim());
        entry(
            &mut s,
            &e(row, 6, label, name, 10, "0 - 9999999999"),
            &|n: &str| if n == name { shown.clone() } else { v(n) },
        );
    }
    entry(&mut s, &e(17, 4, "Text", "TEXT", 40, ""), v);
    keys(&mut s, SUBSET_KEYS);
    bottom_line(&mut s, message, false);
    s
}

/// Subset Member List: `MBR`, `MBRTYPE`, `FROMDATE`, `TODATE`, `TEXT`.
#[must_use]
pub fn subset_mbrs(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 32, "Subset Member List", true);
    let e = |row, label, name, len, hint| Entry {
        row,
        col: 4,
        label,
        end: 23,
        field: 27,
        len,
        name,
        hint: (41, hint),
    };
    entry(
        &mut s,
        &e(5, "Member", "MBR", 10, "*ALL, name, *generic*"),
        v,
    );
    entry(
        &mut s,
        &e(
            7,
            "Member type",
            "MBRTYPE",
            10,
            "*ALL, type, *generic*, *BLANK",
        ),
        v,
    );
    entry(
        &mut s,
        &e(9, "From date", "FROMDATE", 8, "Earliest date to include"),
        v,
    );
    entry(
        &mut s,
        &e(11, "To date", "TODATE", 8, "Latest date to include"),
        v,
    );
    entry(&mut s, &e(13, "Text", "TEXT", 50, ""), v);
    keys(&mut s, SUBSET_KEYS);
    bottom_line(&mut s, message, false);
    s
}

/// Find String: `STRING`, `FROMCOL`, `TOCOL`, `CASE`, `OPTION`,
/// `PROMPT`, `PRTLIST`, `PRTRCDS`, `NUMBER`, `FORMAT`, `MARK`,
/// `OVERFLOW`, `BATCH`, `PARM`.
#[must_use]
pub fn find(v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, 35, "Find String", true);
    let yn = "Y=Yes, N=No";
    let rows: [(usize, usize, &str, &str, usize, &str); 14] = [
        (5, 4, "Find", "STRING", 40, ""),
        (6, 6, "From column number", "FROMCOL", 7, "1 - *RCDLEN"),
        (7, 6, "To column number", "TOCOL", 7, "1 - *RCDLEN"),
        (
            8,
            6,
            "Kind of match",
            "CASE",
            7,
            "1=Same case, 2=Ignore case",
        ),
        (10, 4, "Option", "OPTION", 7, "*NONE, Valid option"),
        (11, 6, "Prompt", "PROMPT", 7, yn),
        (12, 4, "Print list", "PRTLIST", 7, yn),
        (13, 4, "Print records", "PRTRCDS", 7, yn),
        (14, 6, "Number to find", "NUMBER", 7, "*ALL, number"),
        (15, 6, "Print format", "FORMAT", 7, "*CHAR, *HEX, *ALTHEX"),
        (16, 6, "Mark record", "MARK", 7, yn),
        (
            17,
            6,
            "Record overflow",
            "OVERFLOW",
            7,
            "1=Fold, 2=Truncate",
        ),
        (18, 4, "Find string in batch", "BATCH", 7, yn),
        (19, 4, "Parameters", "PARM", 40, ""),
    ];
    for (row, col, label, name, len, hint) in rows {
        entry(
            &mut s,
            &Entry {
                row,
                col,
                label,
                end: 35,
                field: 39,
                len,
                name,
                hint: (49, hint),
            },
            v,
        );
    }
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit"),
            (22, 26, "F5=Refresh"),
            (22, 44, "F12=Cancel"),
            (22, 61, "F16=User options"),
            (23, 2, "F18=Change defaults"),
        ],
    );
    bottom_line(&mut s, message, false);
    s
}

/// Confirm an option on several entries (4=Delete): the entries with
/// their type and text.
#[must_use]
pub fn confirm(what: &str, rows: &[(String, String, String)], message: Option<&str>) -> Screen {
    let mut s = blank();
    let t = format!("Confirm Delete of {what}");
    title(&mut s, 1 + (80 - t.len()) / 2, &t, None);
    put(
        &mut s,
        3,
        2,
        Attr::Blue,
        "Press Enter to confirm your choices for 4=Delete.",
    );
    put(
        &mut s,
        4,
        2,
        Attr::Blue,
        "Press F12=Cancel to return to change your choices.",
    );
    put(&mut s, 6, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 6, 7, Attr::HighIntensity, "Name");
    put(&mut s, 6, 19, Attr::HighIntensity, "Type");
    put(&mut s, 6, 31, Attr::HighIntensity, "Text");
    for (i, (n, t, x)) in rows.iter().take(14).enumerate() {
        put(&mut s, 7 + i, 3, Attr::Normal, "4");
        put(&mut s, 7 + i, 7, Attr::Normal, n);
        put(&mut s, 7 + i, 19, Attr::Normal, t);
        put(
            &mut s,
            7 + i,
            31,
            Attr::Normal,
            &x.chars().take(48).collect::<String>(),
        );
    }
    if rows.len() > 14 {
        more(&mut s, 21, false);
    }
    keys(&mut s, &[(23, 2, "F12=Cancel")]);
    bottom_line(&mut s, message, false);
    s
}

/// Rename or change the text of one entry: `NEW` is the new name or
/// text (`text` true).
#[must_use]
pub fn one_entry(
    heading: &str,
    item: (&str, &str),
    text: bool,
    v: Values<'_>,
    message: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, 1 + (80 - heading.len()) / 2, heading, None);
    put(&mut s, 3, 2, Attr::Blue, "Type choice, press Enter.");
    put(&mut s, 5, 4, Attr::Normal, &leader("Name", 4, 31));
    put(&mut s, 5, 35, Attr::Normal, item.0);
    put(&mut s, 6, 4, Attr::Normal, &leader("Type", 4, 31));
    put(&mut s, 6, 35, Attr::Normal, item.1);
    if text {
        put(&mut s, 8, 4, Attr::Normal, &leader("Text", 4, 31));
        var(&mut s, 8, 35, 44, "NEW", &v("NEW"));
    } else {
        put(&mut s, 8, 4, Attr::Normal, &leader("New name", 4, 31));
        var(&mut s, 8, 35, 10, "NEW", &v("NEW"));
    }
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 14, "F12=Cancel")]);
    bottom_line(&mut s, message, false);
    s
}

/// Create, change or display a user-defined option: `CODE`, `CMD`.
#[must_use]
pub fn user_option(heading: &str, change: bool, v: Values<'_>, message: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 1 + (80 - heading.len()) / 2, heading, None);
    if change {
        put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    }
    put(&mut s, 5, 4, Attr::Normal, &leader("Option", 4, 15));
    put(&mut s, 7, 4, Attr::Normal, &leader("Command", 4, 15));
    if change {
        var(&mut s, 5, 19, 2, "CODE", &v("CODE"));
        var(&mut s, 7, 19, 255, "CMD", &v("CMD"));
    } else {
        put(&mut s, 5, 19, Attr::Normal, &v("CODE"));
        let cmd = v("CMD");
        let chars: Vec<char> = cmd.chars().collect();
        for (i, part) in chars.chunks(60).enumerate() {
            put(
                &mut s,
                7 + i,
                19,
                Attr::Normal,
                &part.iter().collect::<String>(),
            );
        }
    }
    keys(
        &mut s,
        if change {
            &[
                (23, 2, "F3=Exit"),
                (23, 14, "F4=Prompt"),
                (23, 28, "F12=Cancel"),
            ]
        } else {
            &[(23, 2, "F3=Exit"), (23, 14, "F12=Cancel")]
        },
    );
    bottom_line(&mut s, message, false);
    s
}
