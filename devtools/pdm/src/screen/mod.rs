// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PDM's displays, laid out as IBM's display files place them (each
//! position measured on the pub400 goldens): the menu ([`menu`]), the
//! entry panels ([`prompt`]) and the lists ([`list`]). Each builds a
//! `uim::Screen` the job puts on its display; input fields name what
//! they hold with their `Target`.

pub mod list;
pub mod menu;
pub mod prompt;

pub use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target, Text};

/// Rows and columns of PDM's displays.
pub const ROWS: usize = 24;
/// Columns.
pub const COLS: usize = 80;

/// The copyright line's column on the first display of a session.
pub const COPYRIGHT_COL: usize = 43;
/// The copyright line.
pub const COPYRIGHT: &str = "(C) COPYRIGHT QUARTZ SYSTEMS 2026.";

/// A blank 24 by 80 screen.
#[must_use]
pub fn blank() -> Screen {
    Screen::new(ROWS, COLS)
}

/// Text at a position, up to column 80 (PDM writes the last column
/// that the panel renderer leaves alone).
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

/// The title row: the title at its column, the system name at 72 when
/// the display shows it.
pub fn title(s: &mut Screen, col: usize, text: &str, system: Option<&str>) {
    put(s, 1, col, Attr::HighIntensity, text);
    if let Some(sys) = system {
        put(s, 1, 72, Attr::Normal, sys);
    }
}

/// The message line, else the copyright when asked for.
pub fn bottom_line(s: &mut Screen, message: Option<&str>, copyright: bool) {
    match message {
        Some(m) => put(s, ROWS, 2, Attr::HighIntensity, m),
        None if copyright => put(s, ROWS, COPYRIGHT_COL, Attr::HighIntensity, COPYRIGHT),
        None => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaders() {
        assert_eq!(leader("Library", 4, 33).len(), 30);
        assert!(leader("Library", 4, 33).ends_with(". ."));
        assert!(leader("Library", 4, 33).starts_with("Library  ."));
        assert!(leader("ASP number", 4, 33).starts_with("ASP number ."));
        assert!(leader("Save session defaults", 4, 31).starts_with("Save session defaults  ."));
    }
}
