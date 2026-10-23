// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SDA's displays, laid out as IBM's display files place them (each
//! position measured on the pub400 goldens): the menu and the entry
//! displays ([`menu`], [`entry`]), Work with Display Records
//! ([`records`]), Work with Fields and the keyword displays
//! ([`fields`]), the comments editor ([`comments`]), menu design
//! ([`menus`]) and the test facility ([`test`]). Each builds a `uim::Screen`
//! the job puts on its display; input fields name what they hold with
//! their `Target`.

pub mod comments;
pub mod entry;
pub mod fields;
pub mod menu;
pub mod menus;
pub mod records;
pub mod test;

pub use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target, Text};

/// Rows of SDA's displays.
pub const ROWS: usize = 24;
/// Columns.
pub const COLS: usize = 80;

/// The copyright line's column on the menu's first display.
pub const COPYRIGHT_COL: usize = 43;
/// The copyright line.
pub const COPYRIGHT: &str = "(C) COPYRIGHT QUARTZ SYSTEMS 2026.";

/// The values of a display's fields, by name.
pub type Values<'a> = &'a dyn Fn(&str) -> String;

/// A blank 24 by 80 screen.
#[must_use]
pub fn blank() -> Screen {
    Screen::new(ROWS, COLS)
}

/// Text at a position, up to column 80.
pub fn put(s: &mut Screen, row: usize, col: usize, attr: Attr, text: &str) {
    if text.trim().is_empty() || col == 0 || row == 0 || row > s.rows {
        return;
    }
    let room = s.cols + 1 - col;
    s.texts.push(Text {
        row,
        col,
        attr,
        text: text.chars().take(room).collect(),
    });
}

/// An input field.
pub fn input(s: &mut Screen, row: usize, col: usize, len: usize, target: Target, value: &str) {
    s.field(Field {
        row,
        col,
        len,
        attr: Attr::Underline,
        value: value.chars().take(len).collect(),
        target,
        usage: Usage::Inout,
        shift: Shift::None,
        required: false,
        autoskip: false,
    });
}

/// A named input field.
pub fn var(s: &mut Screen, row: usize, col: usize, len: usize, name: &str, value: &str) {
    input(s, row, col, len, Target::Var(name.to_string()), value);
}

/// `label` at its column followed by dot leaders on the columns of
/// `end`'s parity, the last on `end`.
#[must_use]
pub fn leader(label: &str, col: usize, end: usize) -> String {
    let mut out = label.to_string();
    let mut c = col + label.chars().count();
    let mut first = c + 1;
    if first % 2 != end % 2 {
        first += 1;
    }
    while c < first {
        out.push(' ');
        c += 1;
    }
    while c <= end {
        out.push(if (c % 2) == (end % 2) { '.' } else { ' ' });
        c += 1;
    }
    out
}

/// Function keys as `(row, column, text)`.
pub fn keys(s: &mut Screen, keys: &[(usize, usize, &str)]) {
    for (r, c, t) in keys {
        put(s, *r, *c, Attr::Blue, t);
    }
}

/// The title on row 1.
pub fn title(s: &mut Screen, col: usize, text: &str) {
    put(s, 1, col, Attr::HighIntensity, text);
}

/// The title centered as SDA's displays center it.
pub fn title_centered(s: &mut Screen, text: &str) {
    let col = 1 + (COLS - text.chars().count()) / 2;
    title(s, col, text);
}

/// The message on row 24.
pub fn message(s: &mut Screen, message: Option<&str>) {
    if let Some(m) = message {
        put(s, ROWS, 2, Attr::HighIntensity, m);
    }
}

/// `More...` or `Bottom` on a row.
pub fn more(s: &mut Screen, row: usize, bottom: bool) {
    if bottom {
        put(s, row, 74, Attr::HighIntensity, "Bottom");
    } else {
        put(s, row, 73, Attr::HighIntensity, "More...");
    }
    s.more = Some(uim::render::More {
        row,
        col: if bottom { 74 } else { 73 },
        bottom,
    });
}

/// The member heading of the record displays: file, library, member and
/// source type on rows 3 and 4.
pub fn member_head(s: &mut Screen, file: &str, lib: &str, member: &str, srctype: &str) {
    put(s, 3, 2, Attr::Normal, "File  . . . . . . :");
    put(s, 3, 24, Attr::HighIntensity, file);
    put(s, 3, 47, Attr::Normal, "Member . . . . . . :");
    put(s, 3, 70, Attr::HighIntensity, member);
    put(s, 4, 4, Attr::Normal, "Library . . . . :");
    put(s, 4, 26, Attr::HighIntensity, lib);
    put(s, 4, 47, Attr::Normal, "Source type  . . . :");
    put(s, 4, 70, Attr::HighIntensity, srctype);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaders() {
        assert_eq!(leader("Source file", 4, 30), "Source file . . . . . . . .");
        assert!(leader("Member", 4, 30).starts_with("Member  . ."));
        assert!(leader("Library", 6, 30).ends_with(". ."));
    }
}
