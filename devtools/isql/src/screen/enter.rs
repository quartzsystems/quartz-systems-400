// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Enter SQL Statements: the history and the input lines in one
//! scrolling area (rows 4 to 20), the indicator, the two key pages.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, message, more, put, title, var};
use crate::history::{AREA, History, Line, WIDTH};

/// The first row of the area.
pub const AREA_ROW: usize = 4;
/// The input lines' column.
pub const INPUT_COL: usize = 7;

/// The name of input line `i`'s field.
#[must_use]
pub fn line_name(i: usize) -> String {
    format!("LINE{i:02}")
}

/// What the area's rows hold, for the driver: a history line's index
/// or an input line's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// A history line.
    History(usize),
    /// An input line.
    Input(usize),
}

/// The slot each area row shows, from row [`AREA_ROW`].
#[must_use]
pub fn slots(h: &History) -> Vec<Slot> {
    let n = h.lines().len();
    let mut out: Vec<Slot> = (h.top..n).take(AREA).map(Slot::History).collect();
    let mut i = 0;
    while out.len() < AREA && i < h.input.len() {
        out.push(Slot::Input(i));
        i += 1;
    }
    out
}

/// The function keys of page `page` (F24 turns them); the select keys
/// name libraries, files and fields under `*SYS` naming and
/// collections, tables and columns under `*SQL`.
fn key_page(s: &mut Screen, page: u8, sql_naming: bool) {
    if page.is_multiple_of(2) {
        keys(
            s,
            &[
                (22, 2, "F3=Exit"),
                (22, 12, "F4=Prompt"),
                (22, 24, "F6=Insert line"),
                (22, 41, "F9=Retrieve"),
                (22, 55, "F10=Copy line"),
                (23, 2, "F12=Cancel"),
                (23, 24, "F13=Services"),
                (23, 41, "F24=More keys"),
            ],
        );
    } else {
        let (a, b, c) = if sql_naming {
            (
                "F16=Select collections",
                "F17=Select tables",
                "F18=Select columns",
            )
        } else {
            (
                "F16=Select libraries",
                "F17=Select files",
                "F18=Select fields",
            )
        };
        keys(
            s,
            &[
                (22, 2, "F14=Delete line"),
                (22, 20, "F15=Split line"),
                (22, 37, a),
                (23, 2, b),
                (23, 37, c),
                (23, 57, "F24=More keys"),
            ],
        );
    }
}

/// Enter SQL Statements over `h` with key page `page`; the cursor on
/// `cursor` or the first input line.
#[must_use]
pub fn enter(
    h: &History,
    page: u8,
    sql_naming: bool,
    msg: Option<&str>,
    cursor: Option<(usize, usize)>,
) -> Screen {
    let mut s = blank();
    title(&mut s, 31, "Enter SQL Statements");
    put(&mut s, 3, 2, Attr::Blue, "Type SQL statement, press Enter.");
    let lines = h.lines();
    let mut first_input = None;
    for (k, slot) in slots(h).into_iter().enumerate() {
        let row = AREA_ROW + k;
        match slot {
            Slot::History(i) => match &lines[i] {
                Line::Statement { text, first, .. } => {
                    if *first {
                        put(&mut s, row, 5, Attr::Normal, ">");
                    }
                    put(&mut s, row, INPUT_COL, Attr::Normal, text);
                }
                Line::Message { text, .. } => put(&mut s, row, INPUT_COL, Attr::Normal, text),
            },
            Slot::Input(i) => {
                if i == 0 {
                    put(&mut s, row, 2, Attr::Normal, "===>");
                    first_input = Some(row);
                }
                var(&mut s, row, INPUT_COL, WIDTH, &line_name(i), &h.input[i]);
            }
        }
    }
    more(&mut s, 21, 78, h.at_end());
    key_page(&mut s, page, sql_naming);
    message(&mut s, msg);
    s.cursor = cursor.or(first_input.map(|r| (r, INPUT_COL)));
    s
}
