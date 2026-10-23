// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry displays: Design Screens, Design Menus, Test Display File,
//! Recover SDA Session, Add New Record, Subset Records, Change
//! Defaults, Select Database Files, Exit SDA Work Screen, Save DDS -
//! Create Display File and the comments' Exit.

use uim::Screen;
use uim::render::Attr;

use super::{Values, blank, keys, leader, member_head, message, put, title, var};

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
    put(s, e.row, e.col, Attr::Normal, &leader(e.label, e.col, e.end));
    var(s, e.row, e.field, e.len, e.name, &v(e.name));
    put(s, e.row, e.hint.0, Attr::Normal, e.hint.1);
}

const EXIT_PROMPT_CANCEL: &[(usize, usize, &str)] =
    &[(23, 2, "F3=Exit"), (23, 14, "F4=Prompt"), (23, 28, "F12=Cancel")];

const EXIT_REFRESH_CANCEL: &[(usize, usize, &str)] =
    &[(23, 2, "F3=Exit"), (23, 14, "F5=Refresh"), (23, 29, "F12=Cancel")];

/// The member displays behind options 1 and 2: `SRCFILE`, `SRCLIB` and
/// `SRCMBR` (`Member` or `Menu`).
fn specify(title_col: usize, title_text: &str, what: &str, v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, title_col, title_text);
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    for e in [
        Entry {
            row: 5,
            col: 4,
            label: "Source file",
            end: 30,
            field: 34,
            len: 10,
            name: "SRCFILE",
            hint: (47, "Name, F4 for list"),
        },
        Entry {
            row: 7,
            col: 6,
            label: "Library",
            end: 30,
            field: 34,
            len: 10,
            name: "SRCLIB",
            hint: (47, "Name, *LIBL, *CURLIB"),
        },
        Entry {
            row: 9,
            col: 4,
            label: what,
            end: 30,
            field: 34,
            len: 10,
            name: "SRCMBR",
            hint: (47, "Name, F4 for list"),
        },
    ] {
        entry(&mut s, &e, v);
    }
    keys(&mut s, EXIT_PROMPT_CANCEL);
    message(&mut s, msg);
    s.cursor = Some((5, 34));
    s
}

/// Design Screens (option 1).
#[must_use]
pub fn design_screens(v: Values<'_>, msg: Option<&str>) -> Screen {
    specify(33, "Design Screens", "Member", v, msg)
}

/// Design Menus (option 2).
#[must_use]
pub fn design_menus(v: Values<'_>, msg: Option<&str>) -> Screen {
    specify(34, "Design Menus", "Menu", v, msg)
}

/// Test Display File (option 3): `FILE`, `LIB`, `RECORD`, `MORE`.
#[must_use]
pub fn test_dspf(v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 32, "Test Display File");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    for e in [
        Entry {
            row: 5,
            col: 4,
            label: "Display file",
            end: 41,
            field: 45,
            len: 10,
            name: "FILE",
            hint: (58, "Name, F4 for list"),
        },
        Entry {
            row: 6,
            col: 6,
            label: "Library",
            end: 41,
            field: 47,
            len: 10,
            name: "LIB",
            hint: (58, "Name, *LIBL ..."),
        },
        Entry {
            row: 8,
            col: 4,
            label: "Record to be tested",
            end: 41,
            field: 45,
            len: 10,
            name: "RECORD",
            hint: (58, "Name, F4 for list"),
        },
        Entry {
            row: 10,
            col: 4,
            label: "Additional records to display",
            end: 41,
            field: 45,
            len: 10,
            name: "MORE",
            hint: (58, "Name, F4 for list"),
        },
    ] {
        entry(&mut s, &e, v);
    }
    keys(&mut s, EXIT_PROMPT_CANCEL);
    message(&mut s, msg);
    s.cursor = Some((5, 45));
    s
}

/// Recover SDA Session: the member a session ended in without leaving.
#[must_use]
pub fn recover(member: &str, file: &str, lib: &str, choice: &str) -> Screen {
    let mut s = blank();
    title(&mut s, 31, "Recover SDA Session");
    put(&mut s, 3, 2, Attr::Normal, "Member . . . . :");
    put(&mut s, 3, 21, Attr::HighIntensity, member);
    put(&mut s, 4, 2, Attr::Normal, "File . . . . . :");
    put(&mut s, 4, 21, Attr::HighIntensity, file);
    put(&mut s, 5, 4, Attr::Normal, "Library  . . :");
    put(&mut s, 5, 23, Attr::HighIntensity, lib);
    put(&mut s, 7, 2, Attr::Normal, "The previous SDA session for the member shown");
    put(&mut s, 8, 4, Attr::Normal, "abnormally ended.");
    put(&mut s, 10, 2, Attr::Blue, "Select one of the following:");
    put(&mut s, 12, 6, Attr::Normal, "1. Recover changes made in the previous SDA session");
    put(&mut s, 13, 6, Attr::Normal, "2. Discard changes and start a new session");
    put(&mut s, 20, 2, Attr::Normal, "Selection");
    var(&mut s, 21, 6, 1, "CHOICE", choice);
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    s.cursor = Some((21, 6));
    s
}

/// Add New Record: `NAME` and `TYPE`.
#[must_use]
pub fn add_record(
    head: (&str, &str, &str, &str),
    v: Values<'_>,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, 32, "Add New Record");
    member_head(&mut s, head.0, head.1, head.2, head.3);
    put(&mut s, 6, 2, Attr::Blue, "Type choices, press Enter.");
    entry(
        &mut s,
        &Entry {
            row: 8,
            col: 4,
            label: "New record",
            end: 44,
            field: 48,
            len: 10,
            name: "NAME",
            hint: (61, "Name"),
        },
        v,
    );
    entry(
        &mut s,
        &Entry {
            row: 10,
            col: 4,
            label: "Type",
            end: 44,
            field: 48,
            len: 6,
            name: "TYPE",
            hint: (61, "RECORD, USRDFN"),
        },
        v,
    );
    put(&mut s, 11, 61, Attr::Normal, "SFL,    SFLMSG");
    put(&mut s, 12, 61, Attr::Normal, "WINDOW, WDWSFL");
    put(&mut s, 13, 61, Attr::Normal, "PULDWN, PDNSFL");
    put(&mut s, 14, 61, Attr::Normal, "MNUBAR");
    keys(&mut s, EXIT_REFRESH_CANCEL);
    message(&mut s, msg);
    s.cursor = Some((8, 48));
    s
}

/// Subset Records: `NAME` and `TYPE`.
#[must_use]
pub fn subset(head: (&str, &str, &str, &str), v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 33, "Subset Records");
    member_head(&mut s, head.0, head.1, head.2, head.3);
    put(&mut s, 6, 2, Attr::Blue, "Type choices, press Enter.");
    entry(
        &mut s,
        &Entry {
            row: 8,
            col: 4,
            label: "Record name",
            end: 28,
            field: 32,
            len: 10,
            name: "NAME",
            hint: (45, "*ALL, generic*, name"),
        },
        v,
    );
    entry(
        &mut s,
        &Entry {
            row: 10,
            col: 4,
            label: "Record type",
            end: 28,
            field: 32,
            len: 6,
            name: "TYPE",
            hint: (45, "*ALL, SFL*,   RECORD, USRDFN"),
        },
        v,
    );
    put(&mut s, 11, 45, Attr::Normal, "SFL,  SFLCTL, SFLMSG, WINDOW");
    put(&mut s, 12, 45, Attr::Normal, "MNUBAR, PULDWN, GRDRCD");
    keys(&mut s, EXIT_REFRESH_CANCEL);
    message(&mut s, msg);
    s.cursor = Some((8, 32));
    s
}

/// Change Defaults: `FLDDEF`, whether a printed image lists the fields.
#[must_use]
pub fn defaults(v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 33, "Change Defaults");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    put(&mut s, 5, 4, Attr::Normal, "Print work screen image:");
    entry(
        &mut s,
        &Entry {
            row: 6,
            col: 6,
            label: "Include field definitions",
            end: 39,
            field: 43,
            len: 1,
            name: "FLDDEF",
            hint: (48, "Y=Yes, N=No"),
        },
        v,
    );
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((6, 43));
    s
}

/// Select Database Files: four rows of option, file, library, record
/// (`OPTn`, `FILEn`, `LIBn`, `RCDn`).
#[must_use]
pub fn db_files(v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 30, "Select Database Files");
    put(&mut s, 3, 2, Attr::Blue, "Type options and names, press Enter.");
    put(&mut s, 4, 4, Attr::Blue, "1=Display database field list");
    put(&mut s, 5, 4, Attr::Blue, "2=Select all fields for input (I)");
    put(&mut s, 6, 4, Attr::Blue, "3=Select all fields for output (O)");
    put(&mut s, 7, 4, Attr::Blue, "4=Select all fields for both (B) input and output");
    for (col, t) in [(2, "Option"), (12, "Database File"), (28, "Library"), (41, "Record")] {
        put(&mut s, 9, col, Attr::HighIntensity, t);
    }
    for i in 0..4 {
        let row = 10 + i;
        var(&mut s, row, 4, 1, &format!("OPT{i}"), &v(&format!("OPT{i}")));
        var(&mut s, row, 13, 10, &format!("FILE{i}"), &v(&format!("FILE{i}")));
        var(&mut s, row, 28, 10, &format!("LIB{i}"), &v(&format!("LIB{i}")));
        var(&mut s, row, 41, 10, &format!("RCD{i}"), &v(&format!("RCD{i}")));
    }
    keys(&mut s, EXIT_PROMPT_CANCEL);
    message(&mut s, msg);
    s.cursor = Some((10, 4));
    s
}

/// Exit SDA Work Screen, left from the design image.
#[must_use]
pub fn image_exit(choice: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 33, "Exit SDA Work Screen");
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    put(&mut s, 5, 6, Attr::Normal, "1. Save work since last Enter and exit work screen");
    put(&mut s, 6, 6, Attr::Normal, "2. Exit without saving any work done on the work screen");
    put(&mut s, 7, 6, Attr::Normal, "3. Resume work screen session");
    put(&mut s, 20, 2, Attr::Normal, "Selection");
    var(&mut s, 21, 6, 1, "CHOICE", choice);
    keys(&mut s, &[(23, 2, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((21, 6));
    s
}

/// Save DDS - Create Display File: `SAVE`, `SRCFILE`, `SRCLIB`,
/// `SRCMBR`, `TEXT`, `CREATE`, `PROMPT`, `FILE`, `LIB`, `REPLACE`,
/// `BATCH`, `MORE`.
#[must_use]
pub fn save(v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 26, "Save DDS - Create Display File");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    let rows: &[(usize, usize, &str, usize, &str, &str)] = &[
        (5, 4, "Save DDS source", 1, "SAVE", "Y=Yes"),
        (6, 6, "Source file", 10, "SRCFILE", "F4 for list"),
        (7, 8, "Library", 10, "SRCLIB", "Name, *LIBL ..."),
        (8, 6, "Member", 10, "SRCMBR", "F4 for list"),
        (12, 4, "Create display file", 1, "CREATE", "Y=Yes"),
        (13, 6, "Prompt for parameters", 1, "PROMPT", "Y=Yes"),
        (14, 6, "Display file", 10, "FILE", "F4 for list"),
        (15, 8, "Library", 10, "LIB", "Name, *CURLIB"),
        (16, 6, "Replace existing file", 1, "REPLACE", "Y=Yes"),
        (18, 4, "Submit create job in batch", 1, "BATCH", "Y=Yes"),
    ];
    for (row, col, label, len, name, hint) in rows {
        entry(
            &mut s,
            &Entry {
                row: *row,
                col: *col,
                label,
                end: 43,
                field: 47,
                len: *len,
                name,
                hint: (60, hint),
            },
            v,
        );
    }
    put(&mut s, 9, 6, Attr::Normal, &leader("Text", 6, 43));
    var(&mut s, 9, 47, 50, "TEXT", &v("TEXT"));
    put(&mut s, 20, 4, Attr::Normal, "Specify additional");
    put(&mut s, 21, 6, Attr::Normal, &leader("save or create options", 6, 43));
    var(&mut s, 21, 47, 1, "MORE", &v("MORE"));
    put(&mut s, 21, 60, Attr::Normal, "Y=Yes");
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F4=Prompt"), (23, 24, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((5, 47));
    s
}

/// The comments' Exit display: `SAVE`.
#[must_use]
pub fn comments_exit(v: Values<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 38, "Exit");
    put(&mut s, 3, 2, Attr::Blue, "Type choice, press Enter.");
    entry(
        &mut s,
        &Entry {
            row: 5,
            col: 4,
            label: "Save comments",
            end: 38,
            field: 42,
            len: 1,
            name: "SAVE",
            hint: (55, "Y=Yes, N=No"),
        },
        v,
    );
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F5=Refresh"), (23, 25, "F12=Cancel")]);
    s.cursor = Some((5, 42));
    s
}
