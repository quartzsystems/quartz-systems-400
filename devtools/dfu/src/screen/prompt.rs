// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DFU's entry panels: which program, which file, which member. Each
//! panel is a list of entries; a value names its field with its key.

use uim::Screen;
use uim::render::Attr;

use super::{blank, bottom_line, keys, leader, put, var};

/// One entry: row, label, whether it is a library (indented), the
/// field's key, length and legend.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// Row.
    pub row: usize,
    /// Label.
    pub label: &'static str,
    /// Indented under the entry before.
    pub sub: bool,
    /// The field's name.
    pub key: &'static str,
    /// Its length.
    pub len: usize,
    /// The allowed values.
    pub legend: &'static str,
}

const fn e(
    row: usize,
    label: &'static str,
    sub: bool,
    key: &'static str,
    len: usize,
    legend: &'static str,
) -> Entry {
    Entry {
        row,
        label,
        sub,
        key,
        len,
        legend,
    }
}

/// A DFU entry panel.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    /// Title and its column.
    pub title: (usize, &'static str),
    /// The entries.
    pub entries: &'static [Entry],
}

const NAME: &str = "Name, F4 for list";
const LIBS: &str = "Name, *LIBL, *CURLIB";

/// Option 1, then 1: Change a Data File (`CHGDTA`).
pub const RUN_CHANGE: Panel = Panel {
    title: (31, "Change a Data File"),
    entries: &[
        e(5, "Program", false, "PGM", 10, NAME),
        e(6, "Library", true, "PGMLIB", 10, LIBS),
        e(8, "Data file", false, "FILE", 10, "Name, *SAME, F4 for list"),
        e(10, "Library", true, "FILELIB", 10, LIBS),
        e(11, "Member", false, "MBR", 10, "Name, *FIRST, F4 for list"),
    ],
};

/// Option 1, then 2: Display a Data File (`DSPDTA`).
pub const RUN_DISPLAY: Panel = Panel {
    title: (31, "Display a Data File"),
    entries: RUN_CHANGE.entries,
};

/// Option 2: Create a DFU Program.
pub const CREATE: Panel = Panel {
    title: (33, "Create a DFU Program"),
    entries: &[
        e(5, "Program", false, "PGM", 10, NAME),
        e(6, "Library", true, "PGMLIB", 10, "Name, *CURLIB"),
        e(8, "Data file", false, "FILE", 10, NAME),
        e(9, "Library", true, "FILELIB", 10, LIBS),
    ],
};

/// Option 3: Change a DFU Program.
pub const CHANGE: Panel = Panel {
    title: (33, "Change a DFU Program"),
    entries: &[
        e(5, "Program", false, "PGM", 10, NAME),
        e(6, "Library", true, "PGMLIB", 10, LIBS),
        e(8, "Data file", false, "FILE", 10, "Name, *SAME, F4 for list"),
        e(10, "Library", true, "FILELIB", 10, LIBS),
    ],
};

/// Option 4: Delete a DFU Program.
pub const DELETE: Panel = Panel {
    title: (31, "Delete a DFU Program"),
    entries: &[
        e(5, "Program", false, "PGM", 10, NAME),
        e(6, "Library", true, "PGMLIB", 10, LIBS),
    ],
};

/// Option 5: Update Data Using Temporary Program.
pub const TEMP: Panel = Panel {
    title: (23, "Update Data Using Temporary Program"),
    entries: &[
        e(5, "Data file", false, "FILE", 10, NAME),
        e(6, "Library", true, "FILELIB", 10, LIBS),
        e(7, "Member", false, "MBR", 10, "Name, *FIRST, F4 for list"),
    ],
};

/// A panel with its values (`value(key)`) and a message.
#[must_use]
pub fn panel(p: &Panel, value: &dyn Fn(&str) -> String, message: Option<&str>) -> Screen {
    let mut s = blank();
    put(&mut s, 1, p.title.0, Attr::HighIntensity, p.title.1);
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    for en in p.entries {
        let col = if en.sub { 6 } else { 4 };
        put(&mut s, en.row, col, Attr::Normal, &leader(en.label, col, 30));
        var(&mut s, en.row, col + 30, en.len, en.key, &value(en.key));
        put(&mut s, en.row, 48, Attr::Normal, en.legend);
    }
    keys(
        &mut s,
        &[(23, 2, "F3=Exit"), (23, 14, "F4=Prompt"), (23, 28, "F12=Cancel")],
    );
    bottom_line(&mut s, message, false);
    s.cursor = p.entries.first().map(|en| (en.row, 34));
    s
}
