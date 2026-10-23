// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The STRSQL displays, laid out as IBM's display files place them
//! (each position measured on the pub400 goldens). Each builds a
//! `uim::Screen` the job puts on its display; input fields name what
//! they hold with their `Target`.

pub mod data;
pub mod enter;
pub mod lists;
pub mod menus;
pub mod prompt;

pub use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target, Text};

/// Rows of the displays.
pub const ROWS: usize = 24;
/// Columns.
pub const COLS: usize = 80;

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

/// The title, centred as the goldens place it.
pub fn title(s: &mut Screen, col: usize, text: &str) {
    put(s, 1, col, Attr::HighIntensity, text);
}

/// The message line.
pub fn message(s: &mut Screen, message: Option<&str>) {
    if let Some(m) = message {
        put(s, ROWS, 2, Attr::HighIntensity, m);
    }
}

/// `More...` or `Bottom` ending at column `end`.
pub fn more(s: &mut Screen, row: usize, end: usize, bottom: bool) {
    let text = if bottom { "Bottom" } else { "More..." };
    let col = end + 1 - text.len();
    put(s, row, col, Attr::HighIntensity, text);
    s.more = Some(uim::render::More { row, col, bottom });
}
